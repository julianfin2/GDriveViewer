<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  ChevronRight,
  Database,
  ExternalLink,
  File,
  FileImage,
  FileSpreadsheet,
  FileText,
  Folder,
  FolderOpen,
  HardDrive,
  Image,
  KeyRound,
  LogIn,
  LogOut,
  X,
  RefreshCw,
  ScanSearch,
  ShieldCheck,
  SlidersHorizontal,
  Sparkles,
} from "@lucide/vue";
import { openUrl } from "@tauri-apps/plugin-opener";

const DEFAULT_COLUMN_WIDTH = 260;
const MIN_COLUMN_WIDTH = 180;
const MAX_COLUMN_WIDTH = 640;

interface DriveFile {
  id: string;
  name: string;
  mimeType: string;
  modifiedTime?: string;
  size?: string;
  webViewLink?: string;
  iconLink?: string;
  ownerNames: string[];
  parentIds: string[];
  isFolder: boolean;
  isGoogleDoc: boolean;
  hasImages?: boolean | null;
}

interface DriveListResponse {
  parentId: string;
  files: DriveFile[];
}

interface ScanResult {
  folderId: string;
  recursive: boolean;
  totalFiles: number;
  totalDocs: number;
  scannedDocs: number;
  cacheHits: number;
  failedDocs: number;
  docsWithImages: number;
  matches: DriveFile[];
  matchingFolderIds: string[];
  failures: { id: string; name: string; reason: string }[];
}

interface ScanProgress {
  folderId: string;
  totalFiles: number;
  totalDocs: number;
  scannedFolders: number;
  checkedDocs: number;
  scannedDocs: number;
  cacheHits: number;
  failedDocs: number;
  docsWithImages: number;
  currentFileName?: string;
  cancelled: boolean;
  message: string;
}

interface ColumnState {
  parentId: string;
  title: string;
  files: DriveFile[];
  selectedId?: string;
}

const connected = ref(false);
const accessToken = ref("");
const loading = ref(false);
const scanning = ref(false);
const error = ref("");
const currentParentId = ref("root");
const pathStack = ref([{ id: "root", name: "我的云端硬盘" }]);
const files = ref<DriveFile[]>([]);
const selectedId = ref<string>();
const selectedInfo = ref<DriveFile | null>(null);
const onlyDocsWithImages = ref(false);
const scanRecursive = ref(true);
const forceRescan = ref(false);
const scanResult = ref<ScanResult | null>(null);
const scanProgress = ref<ScanProgress | null>(null);
const columns = ref<ColumnState[]>([]);
const folderCache = ref(new Map<string, DriveFile[]>());
const columnHeaderOffset = ref(0);
const columnBodyRef = ref<HTMLElement | null>(null);
const pathbarRef = ref<HTMLElement | null>(null);
const columnWidthReserve = ref(0);
const columnWidths = ref<Record<string, number>>({});
const resizingColumn = ref<{
  parentId: string;
  startX: number;
  startWidth: number;
} | null>(null);

function setError(reason: unknown) {
  error.value = humanizeError(reason);
}

function humanizeError(reason: unknown) {
  const message = String(reason);
  const lower = message.toLowerCase();

  if (lower.includes("401") || lower.includes("unauthorized")) {
    return "访问令牌无效或已经过期，请重新获取 access token 后再连接。";
  }
  if (lower.includes("403") || lower.includes("permission")) {
    return "当前账号没有访问权限，或授权范围不够。请确认 token 包含 Drive 和 Docs 的只读权限。";
  }
  if (lower.includes("404") || lower.includes("not found")) {
    return "没有找到这个文件或文件夹，可能已被删除、移动，或者当前账号无权访问。";
  }
  if (lower.includes("429") || lower.includes("quota") || lower.includes("rate limit")) {
    return "Google API 请求太频繁，触发了限流。请稍等一会儿再试。";
  }
  if (lower.includes("500") || lower.includes("502") || lower.includes("503")) {
    return "Google 服务暂时不可用，请稍后重试。";
  }
  if (
    lower.includes("failed to fetch") ||
    lower.includes("network") ||
    lower.includes("dns") ||
    lower.includes("timed out") ||
    lower.includes("timeout")
  ) {
    return "网络连接失败，请检查网络后重试。";
  }
  if (lower.includes("access token is not configured")) {
    return "还没有连接 Google Drive，请先粘贴 access token 并连接。";
  }
  if (lower.includes("access token cannot be empty")) {
    return "access token 不能为空。";
  }
  if (lower.includes("docs request")) {
    return "检查 Google 文档内容时失败，请确认当前 token 有 Google Docs 只读权限。";
  }
  if (lower.includes("drive request")) {
    return "读取 Google Drive 文件列表失败，请确认当前 token 有 Drive 只读权限。";
  }

  return message;
}

const currentFolderName = computed(
  () => pathStack.value[pathStack.value.length - 1]?.name ?? "我的云端硬盘",
);

onMounted(async () => {
  const unlistenProgress = await listen<ScanProgress>("scan-progress", (event) => {
    scanProgress.value = event.payload;
    if (event.payload.cancelled) {
      scanning.value = false;
    }
  });
  window.addEventListener("beforeunload", () => {
    unlistenProgress();
  });

  try {
    const state = await invoke<{ connected: boolean }>("get_auth_state");
    connected.value = state.connected;
    if (state.connected) {
      await loadFolder("root", "我的云端硬盘", true);
    }
  } catch (reason) {
    setError(reason);
  }
});

async function connect() {
  error.value = "";
  try {
    const state = await invoke<{ connected: boolean }>("set_access_token", {
      token: accessToken.value,
    });
    connected.value = state.connected;
    accessToken.value = "";
    await loadFolder("root", "我的云端硬盘", true);
  } catch (reason) {
    setError(reason);
  }
}

async function disconnect() {
  error.value = "";
  await invoke("clear_access_token");
  connected.value = false;
  files.value = [];
  columns.value = [];
  selectedInfo.value = null;
  folderCache.value.clear();
  scanResult.value = null;
  pathStack.value = [{ id: "root", name: "我的云端硬盘" }];
}

async function loadFolder(id: string, name: string, resetPath = false, force = false) {
  error.value = "";
  const cached = folderCache.value.get(id);

  if (cached && !force) {
    commitFolder(id, name, applyScanMarks(cached), resetPath);
    return;
  }

  loading.value = true;
  try {
    const response = await invoke<DriveListResponse>("list_drive_files", {
      parentId: id,
    });
    folderCache.value.set(response.parentId, response.files);
    commitFolder(response.parentId, name, applyScanMarks(response.files), resetPath);
  } catch (reason) {
    setError(reason);
  } finally {
    loading.value = false;
  }
}

function commitFolder(id: string, name: string, nextFiles: DriveFile[], resetPath: boolean) {
  currentParentId.value = id;
  files.value = nextFiles;
  selectedId.value = undefined;
  selectedInfo.value = null;

  if (resetPath) {
    pathStack.value = [{ id, name }];
  } else {
    const index = pathStack.value.findIndex((item) => item.id === id);
    if (index >= 0) {
      pathStack.value = pathStack.value.slice(0, index + 1);
    } else {
      pathStack.value = [...pathStack.value, { id, name }];
    }
  }

  columns.value = [
    {
      parentId: id,
      title: name,
      files: nextFiles,
    },
  ];
  columnWidthReserve.value = columnsActualWidth(columns.value);

  scrollPathbarToEnd();
}

async function openColumnFile(columnIndex: number, file: DriveFile) {
  selectedId.value = file.id;
  selectedInfo.value = file;
  const previousReserve = currentColumnReserve();
  const nextColumns = columns.value.slice(0, columnIndex + 1);
  nextColumns[columnIndex] = {
    ...nextColumns[columnIndex],
    selectedId: file.id,
  };
  columns.value = nextColumns;
  preserveColumnWidthReserve(nextColumns, previousReserve);

  if (!file.isFolder) {
    selectedId.value = file.id;
    return;
  }

  const cached = folderCache.value.get(file.id);
  if (cached) {
    columns.value = [
      ...nextColumns,
      {
        parentId: file.id,
        title: file.name,
        files: applyScanMarks(cached),
      },
    ];
    preserveColumnWidthReserve(columns.value, previousReserve);
    await scrollColumnsToEnd();
    return;
  }

  loading.value = true;
  error.value = "";
  try {
    const response = await invoke<DriveListResponse>("list_drive_files", {
      parentId: file.id,
    });
    folderCache.value.set(response.parentId, response.files);
    columns.value = [
      ...nextColumns,
      {
        parentId: file.id,
        title: file.name,
        files: applyScanMarks(response.files),
      },
    ];
    preserveColumnWidthReserve(columns.value, previousReserve);
    await scrollColumnsToEnd();
  } catch (reason) {
    setError(reason);
  } finally {
    loading.value = false;
  }
}

async function scanCurrentFolder() {
  scanning.value = true;
  scanProgress.value = null;
  error.value = "";
  try {
    const result = await invoke<ScanResult>("scan_docs_for_images", {
      folderId: currentParentId.value,
      recursive: scanRecursive.value,
      force: forceRescan.value,
    });
    scanResult.value = result;
    scanProgress.value = {
      folderId: result.folderId,
      totalFiles: result.totalFiles,
      totalDocs: result.totalDocs,
      scannedFolders: 0,
      checkedDocs: result.scannedDocs + result.cacheHits + result.failedDocs,
      scannedDocs: result.scannedDocs,
      cacheHits: result.cacheHits,
      failedDocs: result.failedDocs,
      docsWithImages: result.docsWithImages,
      cancelled: false,
      message: "扫描完成",
    };
    files.value = applyScanMarks(files.value);
    folderCache.value = new Map(
      [...folderCache.value.entries()].map(([folderId, cachedFiles]) => [
        folderId,
        applyScanMarks(cachedFiles),
      ]),
    );
    columns.value = columns.value.map((column) => ({
      ...column,
      files: applyScanMarks(column.files),
    }));
  } catch (reason) {
    setError(reason);
  } finally {
    scanning.value = false;
  }
}

async function cancelCurrentScan() {
  await invoke("cancel_scan");
  scanning.value = false;
  if (scanProgress.value) {
    scanProgress.value = {
      ...scanProgress.value,
      cancelled: true,
      message: "正在取消...",
    };
  }
}

function applyScanMarks(source: DriveFile[]) {
  if (!scanResult.value) {
    return source;
  }
  const imageDocIds = new Set(scanResult.value.matches.map((file) => file.id));
  return source.map((file) => ({
    ...file,
    hasImages: file.isGoogleDoc ? imageDocIds.has(file.id) : file.hasImages,
  }));
}

function formatType(file: DriveFile) {
  if (file.isFolder) return "文件夹";
  if (file.isGoogleDoc) return "Google 文档";
  if (file.mimeType.includes("spreadsheet")) return "Google 表格";
  if (file.mimeType.includes("presentation")) return "Google 幻灯片";
  if (file.mimeType.startsWith("image/")) return "图片";
  if (file.mimeType.startsWith("video/")) return "视频";
  if (file.mimeType.startsWith("audio/")) return "音频";

  const typeMap: Record<string, string> = {
    "application/pdf": "PDF",
    "application/zip": "ZIP",
    "application/x-7z-compressed": "7Z",
    "application/x-rar-compressed": "RAR",
    "application/x-compressed": "压缩包",
    "application/x-apple-diskimage": "DMG",
    "application/x-msdownload": "EXE",
    "application/msword": "Word",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document": "Word",
    "application/vnd.ms-excel": "Excel",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet": "Excel",
    "application/vnd.ms-powerpoint": "PPT",
    "application/vnd.openxmlformats-officedocument.presentationml.presentation": "PPT",
    "text/plain": "文本",
    "text/csv": "CSV",
  };

  return typeMap[file.mimeType] ?? "文件";
}

function fileIcon(file: DriveFile) {
  if (file.isFolder) return Folder;
  if (file.isGoogleDoc) return FileText;
  if (file.mimeType.includes("spreadsheet")) return FileSpreadsheet;
  if (file.mimeType.includes("image")) return Image;
  return File;
}

function visibleColumnFiles(column: ColumnState) {
  return column.files.filter((file) => {
    if (onlyDocsWithImages.value) {
      if (file.isFolder) {
        return folderMayContainImageDocs(file.id);
      }
      if (file.hasImages !== true) {
        return false;
      }
    }
    return true;
  });
}

function folderMayContainImageDocs(folderId: string): boolean {
  if (scanResult.value?.matchingFolderIds.includes(folderId)) {
    return true;
  }

  const cachedFiles = folderCache.value.get(folderId);
  if (!cachedFiles) {
    return false;
  }

  return cachedFiles.some((file) => {
    if (file.isFolder) {
      return folderMayContainImageDocs(file.id);
    }
    return file.isGoogleDoc && file.hasImages === true;
  });
}

function imageScanLabel(file: DriveFile) {
  if (!file.isGoogleDoc) {
    return "-";
  }
  return file.hasImages ? "包含图片" : "暂无匹配缓存";
}

async function openSelectedLink() {
  if (!selectedInfo.value?.webViewLink) {
    return;
  }
  await openUrl(selectedInfo.value.webViewLink);
}

function formatDate(value?: string) {
  if (!value) return "-";
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value));
}

function formatSize(value?: string) {
  if (!value) return "-";
  const size = Number(value);
  if (Number.isNaN(size)) return "-";
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  if (size < 1024 * 1024 * 1024) return `${(size / 1024 / 1024).toFixed(1)} MB`;
  return `${(size / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

function syncColumnHeader(event: Event) {
  const element = event.currentTarget as HTMLElement;
  columnHeaderOffset.value = element.scrollLeft;
  trimColumnWidthReserve(element);
}

function columnWidth(parentId: string) {
  return columnWidths.value[parentId] ?? DEFAULT_COLUMN_WIDTH;
}

function columnStyle(parentId: string) {
  const width = `${columnWidth(parentId)}px`;
  return {
    flexBasis: width,
    width,
    minWidth: width,
  };
}

function columnTrackStyle() {
  return {
    minWidth: `${Math.max(columnWidthReserve.value, columnsActualWidth())}px`,
  };
}

function columnsActualWidth(source = columns.value) {
  return source.reduce((width, column) => width + columnWidth(column.parentId), 0);
}

function currentColumnReserve() {
  return Math.max(
    columnWidthReserve.value,
    columnsActualWidth(),
    columnBodyRef.value && columnBodyRef.value.scrollLeft > 0
      ? columnBodyRef.value.scrollLeft + columnBodyRef.value.clientWidth
      : 0,
  );
}

function preserveColumnWidthReserve(source = columns.value, previousReserve = currentColumnReserve()) {
  const actualWidth = columnsActualWidth(source);
  const viewportWidth = columnBodyRef.value?.clientWidth ?? 0;
  const currentScrollLeft = columnBodyRef.value?.scrollLeft ?? 0;

  if (currentScrollLeft <= 0 || actualWidth >= previousReserve || actualWidth <= viewportWidth) {
    columnWidthReserve.value = actualWidth;
    return;
  }

  columnWidthReserve.value = Math.max(actualWidth, previousReserve);
}

function trimColumnWidthReserve(element = columnBodyRef.value) {
  const actualWidth = columnsActualWidth();
  if (!element || columnWidthReserve.value <= actualWidth) {
    return;
  }

  const actualMaxScrollLeft = Math.max(0, actualWidth - element.clientWidth);
  if (element.scrollLeft <= actualMaxScrollLeft + 1) {
    columnWidthReserve.value = actualWidth;
  }
}

function startColumnResize(event: MouseEvent, parentId: string) {
  event.preventDefault();
  event.stopPropagation();
  resizingColumn.value = {
    parentId,
    startX: event.clientX,
    startWidth: columnWidth(parentId),
  };
  window.addEventListener("mousemove", resizeColumn);
  window.addEventListener("mouseup", stopColumnResize, { once: true });
}

function resizeColumn(event: MouseEvent) {
  if (!resizingColumn.value) {
    return;
  }

  const nextWidth = Math.min(
    MAX_COLUMN_WIDTH,
    Math.max(
      MIN_COLUMN_WIDTH,
      resizingColumn.value.startWidth + event.clientX - resizingColumn.value.startX,
    ),
  );

  const previousReserve = currentColumnReserve();
  columnWidths.value = {
    ...columnWidths.value,
    [resizingColumn.value.parentId]: nextWidth,
  };
  preserveColumnWidthReserve(columns.value, previousReserve);
}

function stopColumnResize() {
  resizingColumn.value = null;
  window.removeEventListener("mousemove", resizeColumn);
}

async function scrollColumnsToEnd() {
  await nextTick();
  const element = columnBodyRef.value;
  if (!element) {
    return;
  }
  const actualWidth = columnsActualWidth();
  if (actualWidth <= element.clientWidth) {
    element.scrollLeft = 0;
    columnHeaderOffset.value = 0;
    trimColumnWidthReserve(element);
    return;
  }
  element.scrollLeft = element.scrollWidth;
  columnHeaderOffset.value = element.scrollLeft;
}

async function scrollPathbarToEnd() {
  await nextTick();
  const element = pathbarRef.value;
  if (!element) {
    return;
  }
  element.scrollLeft = element.scrollWidth;
}
</script>

<template>
  <main class="app-shell">
    <aside class="sidebar">
      <div class="brand">
        <div class="brand-mark"><HardDrive :size="23" /></div>
        <div>
          <h1>谷歌云盘查看器</h1>
          <!-- <p>类 Finder 的 Google Drive 浏览器</p> -->
        </div>
      </div>

      <section class="panel auth-panel">
        <label for="token" class="label-with-icon">
          <KeyRound :size="15" />
          <span>Google 访问令牌</span>
        </label>
        <textarea
          id="token"
          v-model="accessToken"
          :disabled="connected"
          placeholder="粘贴 access token"
        />
        <div class="button-row">
          <button v-if="!connected" type="button" @click="connect">
            <LogIn :size="16" />
            <span>连接</span>
          </button>
          <button v-else type="button" class="secondary" @click="disconnect">
            <LogOut :size="16" />
            <span>断开连接</span>
          </button>
        </div>
      </section>

      <section class="panel">
        <div class="panel-title">
          <SlidersHorizontal :size="15" />
          <span>文档图片检查</span>
        </div>
        <label class="setting-row">
          <span class="setting-label">
            <FileImage :size="15" />
            <span>只看有图文档</span>
          </span>
          <input v-model="onlyDocsWithImages" class="switch-input" type="checkbox" />
          <span class="switch-control" aria-hidden="true"></span>
        </label>
        <div class="panel-divider"></div>
        <label class="setting-row">
          <span class="setting-label">
            <FolderOpen :size="15" />
            <span>包含子文件夹</span>
          </span>
          <input v-model="scanRecursive" class="switch-input" type="checkbox" />
          <span class="switch-control" aria-hidden="true"></span>
        </label>
        <label class="setting-row">
          <span class="setting-label">
            <Database :size="15" />
            <span>忽略缓存</span>
          </span>
          <input v-model="forceRescan" class="switch-input" type="checkbox" />
          <span class="switch-control" aria-hidden="true"></span>
        </label>
        <button
          type="button"
          class="wide"
          :disabled="!connected || scanning"
          @click="scanCurrentFolder"
        >
          <ScanSearch :size="16" />
          <span>{{ scanning ? "正在扫描..." : "扫描文档图片" }}</span>
        </button>
        <button
          v-if="scanning"
          type="button"
          class="wide danger"
          @click="cancelCurrentScan"
        >
          <span>取消扫描</span>
        </button>
      </section>

      <section v-if="scanProgress || scanResult" class="panel stats-panel">
        <div class="panel-title">
          <Sparkles :size="15" />
          <span>{{ scanning ? "扫描进度" : "最近扫描" }}</span>
        </div>
        <template v-if="scanProgress">
          <div>
            <span>进度</span>
            <strong>{{ scanProgress.checkedDocs }} / {{ scanProgress.totalDocs }}</strong>
          </div>
          <div><span>文件夹</span><strong>{{ scanProgress.scannedFolders }}</strong></div>
          <div><span>已请求</span><strong>{{ scanProgress.scannedDocs }}</strong></div>
          <div><span>缓存命中</span><strong>{{ scanProgress.cacheHits }}</strong></div>
          <div><span>失败</span><strong>{{ scanProgress.failedDocs }}</strong></div>
          <div><span>包含图片</span><strong>{{ scanProgress.docsWithImages }}</strong></div>
          <div class="scan-status" :class="{ active: scanning, done: !scanning }">
            <span class="scan-status-dot"></span>
            <span>{{ scanProgress.currentFileName || scanProgress.message }}</span>
          </div>
        </template>
        <template v-else-if="scanResult">
          <div><span>文件数</span><strong>{{ scanResult.totalFiles }}</strong></div>
          <div><span>Google 文档</span><strong>{{ scanResult.totalDocs }}</strong></div>
          <div><span>已扫描</span><strong>{{ scanResult.scannedDocs }}</strong></div>
          <div><span>缓存命中</span><strong>{{ scanResult.cacheHits }}</strong></div>
          <div><span>失败</span><strong>{{ scanResult.failedDocs }}</strong></div>
          <div><span>包含图片</span><strong>{{ scanResult.docsWithImages }}</strong></div>
        </template>
      </section>
    </aside>

    <section class="workspace">
      <header class="toolbar">
        <div ref="pathbarRef" class="pathbar">
          <button
            v-for="item in pathStack"
            :key="item.id"
            type="button"
            @click="loadFolder(item.id, item.name, item.id === 'root')"
          >
            <Folder :size="15" />
            {{ item.name }}
          </button>
        </div>
        <div class="toolbar-actions">
          <span class="inline-loading" :class="{ visible: loading }" aria-live="polite">
            <RefreshCw :size="14" />
            <span>正在更新</span>
          </span>
          <button
            type="button"
            class="secondary icon-button"
            :disabled="loading || !connected"
            @click="loadFolder(currentParentId, currentFolderName, true, true)"
          >
            <RefreshCw :size="16" />
            <span>刷新</span>
          </button>
        </div>
      </header>

      <div class="error-slot">
        <div v-if="error" class="error">{{ error }}</div>
      </div>

      <section v-if="!connected" class="empty-state">
        <div class="empty-icon"><ShieldCheck :size="34" /></div>
        <h2>连接 Google Drive</h2>
      </section>

      <section v-else class="browser-surface" :aria-busy="loading || scanning">
        <div class="column-shell" :class="{ dimmed: loading }">
          <div class="column-header">
            <div
              class="column-header-track"
              :style="{
                ...columnTrackStyle(),
                transform: `translateX(${-columnHeaderOffset}px)`,
              }"
            >
              <div
                v-for="column in columns"
                :key="column.parentId"
                class="column-title"
                :style="columnStyle(column.parentId)"
              >
                <span>{{ column.title }}</span>
                <span
                  class="column-resizer"
                  title="拖拽调整宽度"
                  @mousedown="startColumnResize($event, column.parentId)"
                />
              </div>
            </div>
          </div>

          <div ref="columnBodyRef" class="column-body" @scroll="syncColumnHeader">
            <div class="column-view" :style="columnTrackStyle()">
            <div
              v-for="(column, columnIndex) in columns"
              :key="column.parentId"
              class="column"
              :style="columnStyle(column.parentId)"
            >
              <button
                v-for="file in visibleColumnFiles(column)"
                :key="file.id"
                type="button"
                class="column-row"
                :class="{ selected: column.selectedId === file.id }"
                @click="openColumnFile(columnIndex, file)"
              >
                <component :is="fileIcon(file)" :size="17" />
                <span>{{ file.name }}</span>
                <strong v-if="file.hasImages">
                  <Image :size="12" />
                  含图片
                </strong>
                <ChevronRight v-if="file.isFolder" class="chevron" :size="16" />
              </button>
            </div>
            </div>
          </div>
        </div>
      </section>

      <section v-if="selectedInfo" class="selection-info">
        <div class="selection-name">
          <component :is="fileIcon(selectedInfo)" :size="18" />
          <span>{{ selectedInfo.name }}</span>
        </div>
        <div><span>类型</span><strong>{{ formatType(selectedInfo) }}</strong></div>
        <div><span>修改时间</span><strong>{{ formatDate(selectedInfo.modifiedTime) }}</strong></div>
        <div><span>大小</span><strong>{{ formatSize(selectedInfo.size) }}</strong></div>
        <div><span>图片扫描</span><strong>{{ imageScanLabel(selectedInfo) }}</strong></div>
        <button
          type="button"
          class="selection-action"
          title="打开链接"
          :disabled="!selectedInfo.webViewLink"
          @click="openSelectedLink"
        >
          <ExternalLink :size="16" />
        </button>
        <button type="button" class="selection-close" title="关闭" @click="selectedInfo = null">
          <X :size="16" />
        </button>
      </section>

    </section>
  </main>
</template>

<style>
:root {
  color: #1c2430;
  background: #edf0f3;
  font-family:
    Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  font-size: 16px;
  line-height: 1.4;
  font-weight: 400;
  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

* {
  box-sizing: border-box;
}

* {
  scrollbar-width: thin;
  scrollbar-color: #a9b6c4 transparent;
}

*::-webkit-scrollbar {
  width: 8px;
  height: 8px;
  background: transparent;
}

*::-webkit-scrollbar-track {
  background: transparent;
}

*::-webkit-scrollbar-track-piece {
  background: transparent;
}

*::-webkit-scrollbar-thumb {
  border: 2px solid transparent;
  border-radius: 999px;
  background: #a9b6c4;
  background-clip: content-box;
}

*::-webkit-scrollbar-thumb:hover {
  background: #7f91a3;
  background-clip: content-box;
}

*::-webkit-scrollbar-corner {
  background: transparent;
}

*::-webkit-scrollbar-button {
  width: 0;
  height: 0;
  display: none;
  background: transparent;
}


body {
  margin: 0;
  min-width: 960px;
  height: 100vh;
  overflow: hidden;
}

button,
input,
textarea {
  font: inherit;
}

button {
  border: 1px solid #c9d1da;
  background: #ffffff;
  color: #1c2430;
  cursor: pointer;
}

button:disabled {
  cursor: not-allowed;
  opacity: 0.55;
}

.app-shell {
  display: grid;
  grid-template-columns: 280px minmax(0, 1fr);
  height: 100vh;
  overflow: hidden;
}

.sidebar {
  background: #26313d;
  color: #f7fafc;
  padding: 20px;
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-height: 0;
  overflow-y: auto;
}

.brand {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 54px;
}

.brand-mark {
  width: 42px;
  height: 42px;
  border-radius: 8px;
  display: grid;
  place-items: center;
  background: #4ea376;
  color: #ffffff;
  font-weight: 800;
}

.brand-mark svg,
button svg,
.panel-title svg,
.setting-label svg,
.empty-icon svg {
  flex: 0 0 auto;
}

.brand h1,
.brand p,
.empty-state h2,
.empty-state p {
  margin: 0;
}

.brand h1 {
  font-size: 18px;
}

.brand p {
  color: #b9c6d3;
  font-size: 12px;
}

.panel {
  border: 1px solid #3b4856;
  border-radius: 8px;
  padding: 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.panel-title,
.auth-panel label {
  color: #dbe5ee;
  font-size: 13px;
  font-weight: 700;
}

.panel-title,
.label-with-icon {
  display: flex;
  align-items: center;
  gap: 7px;
}

textarea {
  width: 100%;
  min-height: 96px;
  resize: vertical;
  border: 1px solid #536171;
  border-radius: 6px;
  padding: 9px;
  color: #f7fafc;
  background: #1d2630;
}

.button-row {
  display: flex;
  gap: 8px;
}

.button-row button,
.wide {
  width: 100%;
  min-height: 36px;
  border-radius: 6px;
  border: 0;
  background: #4ea376;
  color: #ffffff;
  font-weight: 700;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
}

.button-row .secondary,
.secondary {
  background: #ffffff;
  color: #1c2430;
  border: 1px solid #c9d1da;
}

.wide.danger {
  background: #7f3f3d;
}

.stats-panel div:not(.panel-title) {
  display: flex;
  justify-content: space-between;
  color: #dbe5ee;
  font-size: 13px;
}

.scan-status {
  min-height: 28px;
  margin-top: 2px;
  border: 1px solid #3b4856;
  border-radius: 999px;
  padding: 0 10px;
  display: flex;
  align-items: center;
  gap: 8px;
  color: #dbe5ee;
  background: #1d2630;
  font-size: 12px;
}

.scan-status > span:last-child {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.scan-status-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: #b9c6d3;
}

.scan-status.active {
  border-color: #4ea376;
  color: #f7fafc;
  background: rgba(78, 163, 118, 0.16);
}

.scan-status.active .scan-status-dot {
  background: #4ea376;
  box-shadow: 0 0 0 0 rgba(78, 163, 118, 0.55);
  animation: pulse 1.25s ease-out infinite;
}

.scan-status.done {
  border-color: #536171;
}

@keyframes pulse {
  0% {
    box-shadow: 0 0 0 0 rgba(78, 163, 118, 0.55);
  }
  100% {
    box-shadow: 0 0 0 7px rgba(78, 163, 118, 0);
  }
}

.panel-divider {
  height: 1px;
  margin: 2px 0;
  background: #3b4856;
}

.setting-row {
  position: relative;
  min-height: 32px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  color: #e7eef5;
  font-size: 14px;
  cursor: pointer;
}

.setting-label {
  min-width: 0;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  white-space: nowrap;
}

.switch-input {
  position: absolute;
  inset: 0;
  opacity: 0;
  cursor: pointer;
}

.switch-control {
  position: relative;
  flex: 0 0 auto;
  width: 34px;
  height: 20px;
  border: 1px solid #536171;
  border-radius: 999px;
  background: #1d2630;
  transition:
    border-color 0.16s ease,
    background 0.16s ease;
}

.switch-control::after {
  position: absolute;
  top: 3px;
  left: 3px;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  content: "";
  background: #b9c6d3;
  transition:
    background 0.16s ease,
    transform 0.16s ease;
}

.switch-input:checked + .switch-control {
  border-color: #4ea376;
  background: #4ea376;
}

.switch-input:checked + .switch-control::after {
  background: #ffffff;
  transform: translateX(14px);
}

.setting-row:hover .switch-control {
  border-color: #8ab0ca;
}

.workspace {
  display: grid;
  grid-template-rows: auto auto minmax(0, 1fr) auto;
  min-width: 0;
  min-height: 0;
  position: relative;
  overflow: hidden;
}

.toolbar {
  min-height: 64px;
  padding: 12px 18px;
  border-bottom: 1px solid #d3d9df;
  background: #f9fafb;
  display: flex;
  justify-content: space-between;
  gap: 16px;
  align-items: center;
}

.pathbar,
.toolbar-actions,
.segmented {
  display: flex;
  align-items: center;
  gap: 8px;
}

.pathbar {
  flex: 1 1 auto;
  min-width: 0;
  overflow-x: auto;
  overflow-y: hidden;
  scrollbar-width: none;
}

.pathbar::-webkit-scrollbar {
  display: none;
}

.toolbar-actions {
  flex: 0 0 auto;
}

.pathbar button {
  flex: 0 0 auto;
  border-radius: 6px;
  padding: 7px 10px;
  display: inline-flex;
  align-items: center;
  gap: 7px;
}

.segmented {
  border: 1px solid #c9d1da;
  border-radius: 8px;
  overflow: hidden;
  gap: 0;
}

.segmented button {
  width: 38px;
  height: 34px;
  border: 0;
  border-right: 1px solid #c9d1da;
  display: grid;
  place-items: center;
}

.segmented button:last-child {
  border-right: 0;
}

.segmented .active {
  background: #dce8f1;
}

.error-slot {
  min-height: 0;
  padding: 0 18px;
}

.error {
  margin: 10px 0 0;
  padding: 9px 12px;
  border: 1px solid #e2a09d;
  border-radius: 8px;
  color: #7a2420;
  background: #fff1f0;
  white-space: pre-wrap;
  line-height: 1.4;
  overflow-wrap: anywhere;
}

.empty-state {
  align-self: center;
  justify-self: center;
  max-width: 520px;
  text-align: center;
  color: #536171;
}

.empty-icon {
  width: 58px;
  height: 58px;
  margin: 0 auto 14px;
  border-radius: 8px;
  display: grid;
  place-items: center;
  color: #2d5f80;
  background: #e4edf2;
}

.empty-state h2 {
  color: #1c2430;
  font-size: 28px;
  margin-bottom: 8px;
}

.browser-surface {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  padding: 18px;
  overflow: hidden;
}

.secondary {
  min-height: 34px;
  border-radius: 6px;
  padding: 0 12px;
}

.icon-button {
  display: inline-flex;
  align-items: center;
  gap: 7px;
}

.inline-loading {
  width: 106px;
  min-height: 30px;
  border: 1px solid #c9d1da;
  border-radius: 999px;
  padding: 0 11px;
  display: flex;
  align-items: center;
  gap: 7px;
  color: #536171;
  background: rgba(255, 255, 255, 0.92);
  font-size: 12px;
  font-weight: 700;
  opacity: 0;
  pointer-events: none;
  transform: translateX(4px);
  transition:
    opacity 0.16s ease,
    transform 0.16s ease;
}

.inline-loading.visible {
  opacity: 1;
  transform: translateX(0);
}

.inline-loading svg {
  animation: spin 0.9s linear infinite;
}

.dimmed {
  opacity: 0.68;
  transition: opacity 0.16s ease;
}

.file-scroll {
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
  overflow: auto;
  padding-right: 2px;
  background: #ffffff;
  border-radius: 8px;
}

.list-shell {
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: #ffffff;
  border-radius: 8px;
  border: 1px solid #d3d9df;
}

.list-header {
  flex: 0 0 auto;
  overflow: hidden;
  background: #f4f6f8;
  border-radius: 8px 8px 0 0;
}

.list-body {
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
  overflow: auto;
  background: #ffffff;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.tile-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 10px;
  min-width: 640px;
}

.file-tile {
  position: relative;
  min-height: 132px;
  border-radius: 8px;
  padding: 14px 10px 10px;
  display: grid;
  grid-template-rows: auto auto 1fr;
  justify-items: center;
  gap: 8px;
  text-align: center;
}

.file-tile.selected,
.file-table tr.selected,
.column-row.selected {
  background: #dce8f1;
  border-color: #8ab0ca;
}

.file-icon {
  width: 46px;
  height: 46px;
  border-radius: 8px;
  display: grid;
  place-items: center;
  background: #e6edf3;
  color: #2d5f80;
  font-weight: 800;
}

.file-name {
  width: 100%;
  color: #1c2430;
  font-weight: 650;
  overflow-wrap: anywhere;
}

.file-meta {
  color: #697789;
  font-size: 12px;
}

.badge {
  position: absolute;
  right: 8px;
  top: 8px;
  border-radius: 999px;
  padding: 2px 7px 2px 5px;
  background: #e9f6ef;
  color: #227148;
  font-size: 11px;
  font-weight: 700;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.file-table {
  width: max(100%, 980px);
  border-collapse: separate;
  border-spacing: 0;
  background: #ffffff;
  border: 0;
  border-radius: 0;
  overflow: visible;
}

.file-table-header {
  background: #f4f6f8;
  will-change: transform;
}

.file-table th:nth-child(1),
.file-table td:nth-child(1) {
  width: 42%;
}

.file-table th:nth-child(2),
.file-table td:nth-child(2) {
  width: 13%;
}

.file-table th:nth-child(3),
.file-table td:nth-child(3) {
  width: 24%;
}

.file-table th:nth-child(4),
.file-table td:nth-child(4) {
  width: 10%;
}

.file-table th:nth-child(5),
.file-table td:nth-child(5) {
  width: 11%;
}

.file-table th,
.file-table td {
  height: 47px;
  padding: 0 12px;
  border-bottom: 1px solid #e7ebef;
  text-align: left;
  font-size: 14px;
  vertical-align: middle;
  white-space: nowrap;
}

.file-table td {
  overflow: hidden;
  text-overflow: ellipsis;
}

.file-table th {
  color: #536171;
  background: #f4f6f8;
  font-size: 12px;
  text-transform: uppercase;
}

.row-name {
  min-height: 46px;
  display: flex;
  align-items: center;
  gap: 9px;
  min-width: 0;
  line-height: 1;
}

.row-icon {
  color: #2d5f80;
}

.column-shell {
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid #d3d9df;
  border-radius: 8px;
  background: #ffffff;
}

.column-header {
  flex: 0 0 auto;
  overflow: hidden;
  background: #f4f6f8;
  border-bottom: 1px solid #e7ebef;
}

.column-header-track {
  display: flex;
  width: max-content;
  will-change: transform;
}

.column-body {
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
  overflow: auto;
  background: #ffffff;
}

.column-view {
  display: flex;
  min-width: max-content;
  min-height: 560px;
  background: #ffffff;
}

.column {
  flex: 0 0 auto;
  border-right: 1px solid #d3d9df;
}

.column-title {
  position: relative;
  flex: 0 0 auto;
  padding: 10px 12px;
  border-right: 1px solid #d3d9df;
  color: #536171;
  font-size: 12px;
  font-weight: 800;
  text-transform: uppercase;
  background: #f4f6f8;
}

.column-title > span:first-child {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.column-resizer {
  position: absolute;
  top: 0;
  right: -4px;
  bottom: 0;
  z-index: 3;
  width: 8px;
  cursor: col-resize;
}

.column-resizer::after {
  position: absolute;
  top: 8px;
  right: 3px;
  bottom: 8px;
  width: 1px;
  content: "";
  background: transparent;
}

.column-resizer:hover::after {
  background: #8ab0ca;
}

.column-row {
  width: 100%;
  min-height: 38px;
  border: 0;
  border-bottom: 1px solid #eef1f4;
  border-radius: 0;
  padding: 0 10px;
  display: grid;
  grid-template-columns: 24px minmax(0, 1fr) auto auto;
  align-items: center;
  gap: 8px;
  text-align: left;
}

.column-row > span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.column-row strong {
  color: #227148;
  font-size: 11px;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.chevron {
  color: #697789;
}

.selection-info {
  min-height: 46px;
  border-top: 1px solid #d3d9df;
  background: #f9fafb;
  padding: 0 14px 0 18px;
  display: grid;
  grid-template-columns:
    minmax(140px, 1fr) minmax(84px, 0.5fr) 220px
    minmax(78px, 0.45fr) minmax(120px, 0.65fr) 28px 28px;
  align-items: center;
  column-gap: 6px;
  color: #1c2430;
  font-size: 13px;
}

.selection-info > div {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 7px;
  white-space: nowrap;
}

.selection-info span {
  color: #697789;
}

.selection-info strong {
  min-width: 0;
  overflow: hidden;
  color: #1c2430;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.selection-info > div:nth-child(3) strong {
  overflow: visible;
  text-overflow: clip;
}

.selection-name {
  gap: 9px;
}

.selection-name span {
  min-width: 0;
  overflow: hidden;
  color: #1c2430;
  font-weight: 700;
  text-overflow: ellipsis;
}

.selection-action,
.selection-close {
  width: 28px;
  height: 28px;
  border: 0;
  border-radius: 6px;
  display: grid;
  place-items: center;
  color: #697789;
  background: transparent;
}

.selection-action {
  justify-self: end;
}

.selection-close {
  justify-self: start;
}

.selection-action:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

.selection-action:hover:not(:disabled),
.selection-close:hover {
  color: #1c2430;
  background: #e7ebef;
}

</style>
