use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter};
use tokio::{
    sync::{mpsc, Semaphore},
    time::sleep,
};

const DRIVE_FOLDER_MIME: &str = "application/vnd.google-apps.folder";
const GOOGLE_DOC_MIME: &str = "application/vnd.google-apps.document";
const PAGE_SIZE: usize = 100;
const DOC_SCAN_CONCURRENCY: usize = 6;
const DOC_SCAN_RETRIES: usize = 3;

#[derive(Default)]
struct AppState {
    access_token: Mutex<Option<String>>,
    scan_cache: Mutex<HashMap<String, CachedDocScan>>,
    drive_cache: Mutex<HashMap<String, Vec<DriveFile>>>,
    scan_generation: AtomicU64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveFile {
    id: String,
    name: String,
    mime_type: String,
    modified_time: Option<String>,
    size: Option<String>,
    web_view_link: Option<String>,
    icon_link: Option<String>,
    owner_names: Vec<String>,
    parent_ids: Vec<String>,
    is_folder: bool,
    is_google_doc: bool,
    has_images: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DriveListResponse {
    parent_id: String,
    files: Vec<DriveFile>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthState {
    connected: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanResult {
    folder_id: String,
    recursive: bool,
    total_files: usize,
    total_docs: usize,
    scanned_docs: usize,
    cache_hits: usize,
    failed_docs: usize,
    docs_with_images: usize,
    matches: Vec<DriveFile>,
    matching_folder_ids: Vec<String>,
    failures: Vec<ScanFailure>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanFailure {
    id: String,
    name: String,
    reason: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanProgress {
    folder_id: String,
    total_files: usize,
    total_docs: usize,
    scanned_folders: usize,
    checked_docs: usize,
    scanned_docs: usize,
    cache_hits: usize,
    failed_docs: usize,
    docs_with_images: usize,
    current_file_name: Option<String>,
    cancelled: bool,
    message: String,
}

#[derive(Clone)]
struct CachedDocScan {
    modified_time: Option<String>,
    has_images: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveFilesEnvelope {
    files: Vec<DriveFileRaw>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveFileRaw {
    id: String,
    name: String,
    mime_type: String,
    modified_time: Option<String>,
    size: Option<String>,
    web_view_link: Option<String>,
    icon_link: Option<String>,
    owners: Option<Vec<DriveOwner>>,
    parents: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveOwner {
    display_name: Option<String>,
    email_address: Option<String>,
}

impl From<DriveFileRaw> for DriveFile {
    fn from(value: DriveFileRaw) -> Self {
        let owner_names = value
            .owners
            .unwrap_or_default()
            .into_iter()
            .filter_map(|owner| owner.display_name.or(owner.email_address))
            .collect();
        let is_folder = value.mime_type == DRIVE_FOLDER_MIME;
        let is_google_doc = value.mime_type == GOOGLE_DOC_MIME;

        Self {
            id: value.id,
            name: value.name,
            mime_type: value.mime_type,
            modified_time: value.modified_time,
            size: value.size,
            web_view_link: value.web_view_link,
            icon_link: value.icon_link,
            owner_names,
            parent_ids: value.parents.unwrap_or_default(),
            is_folder,
            is_google_doc,
            has_images: None,
        }
    }
}

#[tauri::command]
fn set_access_token(state: tauri::State<'_, AppState>, token: String) -> Result<AuthState, String> {
    let cleaned = token.trim().to_string();
    if cleaned.is_empty() {
        return Err("Access token cannot be empty.".into());
    }

    *state
        .access_token
        .lock()
        .map_err(|_| "Failed to update auth state.")? = Some(cleaned);
    Ok(AuthState { connected: true })
}

#[tauri::command]
fn clear_access_token(state: tauri::State<'_, AppState>) -> Result<AuthState, String> {
    *state
        .access_token
        .lock()
        .map_err(|_| "Failed to update auth state.")? = None;
    state
        .scan_cache
        .lock()
        .map_err(|_| "Failed to clear scan cache.")?
        .clear();
    state
        .drive_cache
        .lock()
        .map_err(|_| "Failed to clear Drive cache.")?
        .clear();
    state.scan_generation.fetch_add(1, Ordering::SeqCst);
    Ok(AuthState { connected: false })
}

#[tauri::command]
fn get_auth_state(state: tauri::State<'_, AppState>) -> Result<AuthState, String> {
    let connected = state
        .access_token
        .lock()
        .map_err(|_| "Failed to read auth state.")?
        .is_some();
    Ok(AuthState { connected })
}

#[tauri::command]
async fn list_drive_files(
    state: tauri::State<'_, AppState>,
    parent_id: Option<String>,
) -> Result<DriveListResponse, String> {
    let token = read_token(&state)?;
    let parent = parent_id.unwrap_or_else(|| "root".to_string());
    let client = Client::new();
    let files = list_children(&client, &token, &parent).await?;
    state
        .drive_cache
        .lock()
        .map_err(|_| "Failed to update Drive cache.")?
        .insert(parent.clone(), files.clone());

    Ok(DriveListResponse {
        parent_id: parent,
        files,
    })
}

#[tauri::command]
fn cancel_scan(state: tauri::State<'_, AppState>) {
    state.scan_generation.fetch_add(1, Ordering::SeqCst);
}

#[tauri::command]
async fn scan_docs_for_images(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    folder_id: String,
    recursive: bool,
    force: bool,
) -> Result<ScanResult, String> {
    let token = read_token(&state)?;
    let client = Client::new();
    let scan_generation = state.scan_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let root = if folder_id.trim().is_empty() {
        "root".to_string()
    } else {
        folder_id
    };
    emit_scan_progress(
        &app,
        ScanProgress {
            folder_id: root.clone(),
            total_files: 0,
            total_docs: 0,
            scanned_folders: 0,
            checked_docs: 0,
            scanned_docs: 0,
            cache_hits: 0,
            failed_docs: 0,
            docs_with_images: 0,
            current_file_name: None,
            cancelled: false,
            message: "正在读取目录...".into(),
        },
    );

    let all_files = collect_files(
        &app,
        &client,
        &token,
        &state,
        &root,
        recursive,
        scan_generation,
    )
    .await?;
    let docs: Vec<DriveFile> = all_files
        .iter()
        .filter(|file| file.is_google_doc)
        .cloned()
        .collect();
    let total_docs = docs.len();

    let mut cache_hits = 0;
    let mut to_scan = Vec::new();
    let mut matches = Vec::new();
    let mut checked_docs = 0;

    {
        let cache = state
            .scan_cache
            .lock()
            .map_err(|_| "Failed to read scan cache.")?;

        for doc in docs {
            let cached = cache.get(&doc.id);
            if !force
                && cached
                    .map(|entry| entry.modified_time == doc.modified_time)
                    .unwrap_or(false)
            {
                cache_hits += 1;
                checked_docs += 1;
                if cached.map(|entry| entry.has_images).unwrap_or(false) {
                    let mut matched = doc;
                    matched.has_images = Some(true);
                    matches.push(matched);
                }
                emit_scan_progress(
                    &app,
                    ScanProgress {
                        folder_id: root.clone(),
                        total_files: all_files.len(),
                        total_docs,
                        scanned_folders: 0,
                        checked_docs,
                        scanned_docs: 0,
                        cache_hits,
                        failed_docs: 0,
                        docs_with_images: matches.len(),
                        current_file_name: None,
                        cancelled: false,
                        message: "使用缓存结果".into(),
                    },
                );
            } else {
                to_scan.push(doc);
            }
        }
    }

    let semaphore = Arc::new(Semaphore::new(DOC_SCAN_CONCURRENCY));
    let (scan_tx, mut scan_rx) = mpsc::channel(to_scan.len().max(1));

    for doc in to_scan {
        let client = client.clone();
        let token = token.clone();
        let semaphore = semaphore.clone();
        let doc_name = doc.name.clone();
        let scan_tx = scan_tx.clone();

        tauri::async_runtime::spawn(async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .map_err(|_| "Scan worker was closed.".to_string())?;
            let result = document_has_images(&client, &token, &doc.id).await;
            let _ = scan_tx.send(Ok::<_, String>((doc, doc_name, result))).await;
            Ok::<_, String>(())
        });
    }
    drop(scan_tx);

    let mut scanned_docs = 0;
    let mut failed_docs = 0;
    let mut failures = Vec::new();
    while let Some(scan_message) = scan_rx.recv().await {
        if scan_cancelled(&state, scan_generation) {
            emit_scan_progress(
                &app,
                ScanProgress {
                    folder_id: root.clone(),
                    total_files: all_files.len(),
                    total_docs,
                    scanned_folders: 0,
                    checked_docs,
                    scanned_docs,
                    cache_hits,
                    failed_docs,
                    docs_with_images: matches.len(),
                    current_file_name: None,
                    cancelled: true,
                    message: "扫描已取消".into(),
                },
            );
            break;
        }

        let (mut doc, doc_name, scan_result) = scan_message?;
        checked_docs += 1;

        match scan_result {
            Ok(has_images) => {
                scanned_docs += 1;

                state
                    .scan_cache
                    .lock()
                    .map_err(|_| "Failed to update scan cache.")?
                    .insert(
                        doc.id.clone(),
                        CachedDocScan {
                            modified_time: doc.modified_time.clone(),
                            has_images,
                        },
                    );

                doc.has_images = Some(has_images);
                if has_images {
                    matches.push(doc);
                }
            }
            Err(reason) => {
                failed_docs += 1;
                failures.push(ScanFailure {
                    id: doc.id,
                    name: doc.name,
                    reason,
                });
            }
        }

        emit_scan_progress(
            &app,
            ScanProgress {
                folder_id: root.clone(),
                total_files: all_files.len(),
                total_docs,
                scanned_folders: 0,
                checked_docs,
                scanned_docs,
                cache_hits,
                failed_docs,
                docs_with_images: matches.len(),
                current_file_name: Some(doc_name),
                cancelled: false,
                message: "正在检查文档".into(),
            },
        );
    }

    matches.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    let matching_folder_ids = collect_matching_folder_ids(&all_files, &matches);

    Ok(ScanResult {
        folder_id: root,
        recursive,
        total_files: all_files.len(),
        total_docs,
        scanned_docs,
        cache_hits,
        failed_docs,
        docs_with_images: matches.len(),
        matches,
        matching_folder_ids,
        failures,
    })
}

fn collect_matching_folder_ids(all_files: &[DriveFile], matches: &[DriveFile]) -> Vec<String> {
    let parent_map: HashMap<&str, Vec<&str>> = all_files
        .iter()
        .map(|file| {
            (
                file.id.as_str(),
                file.parent_ids
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let mut folder_ids = HashSet::new();

    for matched in matches {
        let mut queue: VecDeque<&str> = matched.parent_ids.iter().map(String::as_str).collect();
        while let Some(folder_id) = queue.pop_front() {
            if !folder_ids.insert(folder_id.to_string()) {
                continue;
            }

            if let Some(parent_ids) = parent_map.get(folder_id) {
                queue.extend(parent_ids.iter().copied());
            }
        }
    }

    folder_ids.into_iter().collect()
}

fn read_token(state: &tauri::State<'_, AppState>) -> Result<String, String> {
    state
        .access_token
        .lock()
        .map_err(|_| "Failed to read auth state.")?
        .clone()
        .ok_or_else(|| "Google access token is not configured.".to_string())
}

async fn list_children(
    client: &Client,
    token: &str,
    parent_id: &str,
) -> Result<Vec<DriveFile>, String> {
    let query = format!("'{parent_id}' in parents and trashed = false");
    let encoded_query = utf8_percent_encode(&query, NON_ALPHANUMERIC).to_string();
    let fields = "nextPageToken,files(id,name,mimeType,modifiedTime,size,webViewLink,iconLink,owners(displayName,emailAddress),parents)";
    let encoded_fields = utf8_percent_encode(fields, NON_ALPHANUMERIC).to_string();
    let mut page_token: Option<String> = None;
    let mut files = Vec::new();

    loop {
        let mut url = format!(
            "https://www.googleapis.com/drive/v3/files?q={encoded_query}&pageSize={PAGE_SIZE}&fields={encoded_fields}&orderBy=folder,name_natural"
        );
        if let Some(token) = &page_token {
            url.push_str("&pageToken=");
            url.push_str(&utf8_percent_encode(token, NON_ALPHANUMERIC).to_string());
        }

        let response = client
            .get(url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|error| format!("Drive request failed: {error}"))?;

        if !response.status().is_success() {
            return Err(format_google_error("Drive request", response).await);
        }

        let envelope = response
            .json::<DriveFilesEnvelope>()
            .await
            .map_err(|error| format!("Failed to decode Drive response: {error}"))?;

        files.extend(envelope.files.into_iter().map(DriveFile::from));
        page_token = envelope.next_page_token;
        if page_token.is_none() {
            break;
        }
    }

    Ok(files)
}

async fn collect_files(
    app: &AppHandle,
    client: &Client,
    token: &str,
    state: &tauri::State<'_, AppState>,
    root: &str,
    recursive: bool,
    scan_generation: u64,
) -> Result<Vec<DriveFile>, String> {
    let mut collected = Vec::new();
    let mut queue = VecDeque::from([root.to_string()]);
    let mut scanned_folders = 0;
    let mut discovered_docs = 0;

    while let Some(folder_id) = queue.pop_front() {
        if scan_cancelled(state, scan_generation) {
            return Ok(collected);
        }

        let children = list_children_cached(client, token, state, &folder_id).await?;
        scanned_folders += 1;
        discovered_docs += children.iter().filter(|file| file.is_google_doc).count();
        if recursive {
            queue.extend(
                children
                    .iter()
                    .filter(|file| file.is_folder)
                    .map(|file| file.id.clone()),
            );
        }
        collected.extend(children);

        emit_scan_progress(
            app,
            ScanProgress {
                folder_id: root.to_string(),
                total_files: collected.len(),
                total_docs: discovered_docs,
                scanned_folders,
                checked_docs: 0,
                scanned_docs: 0,
                cache_hits: 0,
                failed_docs: 0,
                docs_with_images: 0,
                current_file_name: None,
                cancelled: false,
                message: format!("正在读取目录，已读取 {scanned_folders} 个文件夹"),
            },
        );
    }

    Ok(collected)
}

async fn list_children_cached(
    client: &Client,
    token: &str,
    state: &tauri::State<'_, AppState>,
    parent_id: &str,
) -> Result<Vec<DriveFile>, String> {
    if let Some(files) = state
        .drive_cache
        .lock()
        .map_err(|_| "Failed to read Drive cache.")?
        .get(parent_id)
        .cloned()
    {
        return Ok(files);
    }

    let files = list_children(client, token, parent_id).await?;
    state
        .drive_cache
        .lock()
        .map_err(|_| "Failed to update Drive cache.")?
        .insert(parent_id.to_string(), files.clone());
    Ok(files)
}

fn scan_cancelled(state: &tauri::State<'_, AppState>, scan_generation: u64) -> bool {
    state.scan_generation.load(Ordering::SeqCst) != scan_generation
}

fn emit_scan_progress(app: &AppHandle, progress: ScanProgress) {
    let _ = app.emit("scan-progress", progress);
}

async fn document_has_images(
    client: &Client,
    token: &str,
    document_id: &str,
) -> Result<bool, String> {
    let fields =
        utf8_percent_encode("inlineObjects,positionedObjects", NON_ALPHANUMERIC).to_string();
    let url = format!("https://docs.googleapis.com/v1/documents/{document_id}?fields={fields}");
    let mut last_error = None;

    for attempt in 0..DOC_SCAN_RETRIES {
        let response = client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|error| format!("Docs request failed: {error}"))?;

        let status = response.status();
        if status.is_success() {
            let document = response
                .json::<Value>()
                .await
                .map_err(|error| format!("Failed to decode Docs response: {error}"))?;

            return Ok(object_map_contains_image(document.get("inlineObjects"))
                || object_map_contains_image(document.get("positionedObjects")));
        }

        let should_retry = status.as_u16() == 429 || status.is_server_error();
        let error = format_google_error("Docs request", response).await;
        if !should_retry || attempt + 1 >= DOC_SCAN_RETRIES {
            return Err(error);
        }

        last_error = Some(error);
        sleep(Duration::from_millis(350 * 2_u64.pow(attempt as u32))).await;
    }

    Err(last_error.unwrap_or_else(|| "Docs request failed.".into()))
}

fn object_map_contains_image(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_object)
        .map(|objects| {
            objects.values().any(|object| {
                object
                    .pointer("/inlineObjectProperties/embeddedObject/imageProperties")
                    .is_some()
                    || object
                        .pointer("/positionedObjectProperties/embeddedObject/imageProperties")
                        .is_some()
            })
        })
        .unwrap_or(false)
}

async fn format_google_error(context: &str, response: reqwest::Response) -> String {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if body.trim().is_empty() {
        format!("{context} failed with status {status}.")
    } else {
        format!("{context} failed with status {status}: {body}")
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            set_access_token,
            clear_access_token,
            get_auth_state,
            list_drive_files,
            cancel_scan,
            scan_docs_for_images
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
