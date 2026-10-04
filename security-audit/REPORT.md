# GDriveViewer 安全审核报告

审核日期：2026-10-04。审核对象：本地工作区及本地 Git 引用可达历史。没有提交 Git 变更，也没有发布、启动应用或启动前端开发服务器。

## 1. 项目概况与范围

项目使用 Vue 3、TypeScript、Vite、Tauri 2、Rust 2021、Tokio、reqwest/rustls。包管理器为 pnpm 与 Cargo；依赖清单为 package.json、pnpm-workspace.yaml、pnpm-lock.yaml、src-tauri/Cargo.toml、src-tauri/Cargo.lock。发布目标为 Windows MSI 和 macOS APP/DMG。

共 17 个直接依赖声明：npm 9 个（运行时 4、开发 5），Cargo 8 个（运行时 7、构建 1）。pnpm 锁文件有 109 个包条目（含平台可选包）；Cargo 外部包条目从 502 个降为 489 个。数量按包/版本条目统计，不能理解为所有平台都会执行全部依赖。

逐文件审查了第一方源码、Tauri 配置与 capabilities、构建入口、两个 GitHub Actions 工作流、Dependabot、类型配置、README、忽略规则和 SVG；二进制图标进行了凭据特征检查。密钥扫描另覆盖前端 dist，全部本地引用可达的 25 个提交、66 个去重历史 blob。当前源码/配置/文档/图标及 dist 共扫描 38 个文件，未发现凭据特征命中。

依赖审核采用逐包 WebSearch（全部直接依赖及关键间接依赖）、官方安全公告、GitHub Advisory Database、RustSec、pnpm audit 和 OSV 批量查询。初次 pnpm audit 有 6 条公告（High 4、Medium 2、Critical 0），最终为 0。Cargo 最终 489 个条目全部查询 OSV；剩下 glib、quick-xml、proc-macro-error 三个包的风险/维护公告。已人工补查 OSV 未返回的最新 Tauri IPC 公告，因此不能只以扫描器结果判定安全。本机未安装 cargo-audit，使用 OSV/RustSec 交叉核对，没有将未运行的工具写成通过。

环境：Node v26.8.2、pnpm 11.21.0、Rust/Cargo 1.97.1。项目原先没有测试脚本或 Rust 测试，本次增加必要的安全回归测试。

完整前后版本清单及 Cargo 依赖关系见 [dependencies.json](dependencies.json)，扫描结果、官方 OSV 公告原文和 Action 提交解析结果见 [evidence.json](evidence.json)。

## 2. 全部直接依赖

| 生态/用途 | 包 | 审核前实际版本 | 修复后实际版本 | 公告核对结果 |
| --- | --- | --- | --- | --- |
| npm 运行时 | @lucide/vue | 1.16.0 | 1.16.0 | 未发现适用公告；仓库元数据指向官方 lucide-icons/lucide，未发现仿冒证据 |
| npm 运行时 | @tauri-apps/api | 2.11.0 | 2.12.1 | 与 Rust Tauri 同步小版本；IPC 漏洞主体位于 Rust crate |
| npm 运行时 | @tauri-apps/plugin-opener | 2.5.4 | 2.7.0 | 与 Rust 插件同步小版本；另收紧能力范围 |
| npm 运行时 | vue | 3.5.34 | 3.5.34 | 未发现适用的直接漏洞；其编译依赖 PostCSS/nanoid 已修复 |
| npm 开发 | @tauri-apps/cli | 2.11.2 | 2.12.1 | 未发现适用的直接公告；与 Tauri 版本同步 |
| npm 开发 | @vitejs/plugin-vue | 6.0.7 | 6.0.7 | 未发现适用的直接公告；其 Vite/PostCSS 路径已修复 |
| npm 开发 | typescript | 6.0.3 | 6.0.3 | 未发现适用公告 |
| npm 开发 | vite | 6.4.2 | 6.4.3 | 修复 Windows 路径绕过、NTLM 信息泄露 |
| npm 开发 | vue-tsc | 3.2.9 | 3.2.9 | 未发现适用公告；本项目不包含旧 Vue 2 的 vue-template-compiler 漏洞包 |
| Cargo 构建 | tauri-build | 2.6.2 | 2.7.1 | 上游兼容小版本联动；间接 quick-xml 风险保留 |
| Cargo 运行时 | tauri | 2.11.2 | 2.12.1 | 高危 IPC 修复要求至少 2.11.6；实际解析到兼容 2.12.1 |
| Cargo 运行时 | tauri-plugin-opener | 2.5.4 | 2.7.0 | 兼容联动；未发现适用的直接公告 |
| Cargo 运行时 | serde | 1.0.228 | 1.0.228 | 未发现适用公告 |
| Cargo 运行时 | serde_json | 1.0.149 | 1.0.149 | 未发现适用公告 |
| Cargo 运行时 | reqwest | 0.12.28 | 0.12.28 | 未发现适用的直接公告；底层 rustls 已修复，关闭默认 features，启用 rustls-tls |
| Cargo 运行时 | tokio | 1.52.3 | 1.52.3 | 未发现适用公告；显式启用取消/测试所需 macros、rt、time |
| Cargo 运行时 | percent-encoding | 2.3.2 | 2.3.2 | 未发现适用公告；URL 编码不能代替 Drive 查询语法校验，见 C02 |

“未发现适用公告”仅代表本次查询结果，不等于不存在漏洞。旧公告不因名称相似而套用：本项目没有 tauri-plugin-shell，也未使用 Deno esbuild 入口。

## 3. 关键间接依赖实际版本

| 包 | 审核前 | 修复后 | 说明 |
| --- | --- | --- | --- |
| postcss | 8.5.14 | 8.5.28 | workspace override 保证 >=8.5.23 的兼容 8.x 修复版本 |
| nanoid | 3.3.12 | 3.3.19 | workspace override 保证 >=3.3.18 的兼容 3.x 修复版本 |
| rollup | 4.60.4 | 4.60.4 | 已超出核对的已知受影响范围 |
| esbuild | 0.25.12 | 0.25.12 | 没有调用 esbuild serve；Windows 文件读取公告列出的版本为 0.27.3，未按模糊范围误报 |
| picomatch | 4.0.4 | 4.0.4 | 已包含核对的 glob 漏洞修复 |
| entities | 7.0.1 | 7.0.1 | 本次未发现适用公告 |
| magic-string | 0.30.21 | 0.30.21 | 本次未发现适用公告 |
| rustls | 0.23.40 | 0.23.45 | TLS 握手验证修复 |
| rustls-webpki | 0.103.13 | 0.103.15 | 随 rustls 兼容升级 |
| anyhow | 1.0.102 | 1.0.103 | 修复 downcast_mut 不健全行为 |
| event-listener | 5.4.1 | 5.4.2 | 修复非 Send 标签跨线程问题 |
| serde_with / serde_with_macros | 3.20.0 | 3.21.0 | 修复 KeyValueMap 空元素 panic |
| quinn-proto | 0.11.14 | 0.11.15 | 修复乱序流重组无界内存分配；Windows 活跃依赖树无该包 |
| quick-xml | 0.39.4 | 0.39.4 | 按开发者决定保留；由 plist 1.9.0 -> tauri-utils 引入 |
| glib | 0.18.5 | 0.18.5 | 按开发者决定保留；Linux GTK 依赖链 |
| proc-macro-error | 1.0.4 | 1.0.4 | 停止维护，非当前 Windows 活跃依赖 |
| time | 0.3.47 | 0.3.47 | 本次 OSV 查询未发现适用公告 |
| ring | 0.17.14 | 0.17.14 | 本次 OSV 查询未发现适用公告 |
| hyper | 1.9.0 | 1.9.0 | 本次 OSV 查询未发现适用公告 |
| wry / tao | 0.55.1 / 0.35.2 | 0.57.0 / 0.37.1 | Tauri 的兼容上游联动依赖，未绕过上游约束强行替换 |
| tauri-utils | 2.9.2 | 2.10.1 | Tauri 联动；移除旧 UNIC 依赖链 |
| reqwest（另一版本） | 0.13.3 | 0.13.3 | 锁文件间接条目，区别于应用直接使用的 0.12.28 |

UNIC 的 unic-char-range、unic-common、unic-char-property、unic-ucd-version、unic-ucd-ident 原锁定均为 0.9.0，分别有停止维护公告，本次已随上游升级移除。没有通过替换包名或引入自定义补丁掩盖公告。

GTK3 的停更公告已于 2026-09 撤回，官方仓库恢复开发，不能继续将 gtk/gdk 当前生态描述为“已停止维护”。glib 0.18.5 的具体内存安全公告仍然有效。[RustSec 撤回说明](https://rustsec.org/advisories/RUSTSEC-2024-0412.html)

Vite 6.4 仍接受安全补丁，不必为本次修复强行升级至 Vite 8。[官方支持范围](https://vite.dev/releases)

## 4. 问题汇总

位置均指修复后的文件。依赖公告按漏洞本身评级；没有 CVSS 的不健全行为/维护问题使用审核评级，并在详情中说明。

| 编号 | 类别 | 严重级别 | 位置 | 状态 |
| --- | --- | --- | --- | --- |
| D01 | 依赖 | High | package.json:23；pnpm-lock.yaml（vite） | 已修复 |
| D02 | 依赖 | High | pnpm-workspace.yaml:4（postcss） | 已修复 |
| D03 | 依赖 | High | pnpm-workspace.yaml:5（nanoid） | 已修复 |
| D04 | 依赖 | High | src-tauri/Cargo.toml:21；Cargo.lock（tauri） | 已修复 |
| D05 | 依赖 | Medium | src-tauri/Cargo.lock:2955（rustls） | 已修复 |
| D06 | 依赖 | High | src-tauri/Cargo.lock:2681（quinn-proto） | 已修复 |
| D07 | 依赖 | Medium | src-tauri/Cargo.lock:3204（serde_with） | 已修复 |
| D08 | 依赖 | Medium | src-tauri/Cargo.lock:45（anyhow） | 已修复；审核评级 |
| D09 | 依赖 | Medium | src-tauri/Cargo.lock:895（event-listener） | 已修复；审核评级 |
| D10 | 依赖 | High | src-tauri/Cargo.lock:2652（quick-xml） | 建议关注；开发者已决定保留 |
| D11 | 依赖 | Medium | src-tauri/Cargo.lock:1289（glib） | 建议关注；开发者已决定保留；审核评级 |
| D12 | 依赖 | Low | src-tauri/Cargo.lock:2619（proc-macro-error） | 建议关注；维护风险 |
| C01 | 代码 | Medium | src-tauri/src/lib.rs:37、53、236；src/App.vue:212、234、251 | 已修复 |
| C02 | 代码 | Medium | src-tauri/src/lib.rs:545、557 | 已修复 |
| C03 | 代码 | Medium | src-tauri/tauri.conf.json:22 | 已修复；纵深防御缺失 |
| C04 | 代码 | Medium | src-tauri/capabilities/default.json:9；src/security.ts:1 | 已修复 |
| C05 | 代码 | Medium | src-tauri/src/lib.rs:363、623；src/App.vue（目录递归） | 已修复 |
| C06 | 代码 | High | .github/workflows/release.yml:73 | 已修复 |
| C07 | 代码 | Medium | .github/workflows/codeql.yml、release.yml（全部 uses） | 已修复；供应链加固 |
| C08 | 代码 | Low | src-tauri/src/lib.rs:535、771 | 已修复；网络/错误信息加固 |
| S01 | 密钥 | Low | .gitignore:14；src/App.vue:688 | 已修复；忽略规则/输入保护，不代表发现泄露 |

## 5. 依赖问题详情与实际可达性

- **D01 Vite**：Windows 下开发服务的路径拒绝规则可绕过，读取被拒绝的文件；网络利用要求主动将 dev server 暴露到网络，配置中的 TAURI_DEV_HOST 可开启这一条件。另一公告涉及 Windows UNC 路径触发 NTLMv2 信息泄露，恶意网站请求本机编辑器中间件也可能触发。升级 6.4.2 -> 6.4.3，未改大版本，也没有为验证启动开发服务。[路径公告](https://github.com/vitejs/vite/security/advisories/GHSA-fx2h-pf6j-xcff)、[NTLM 公告](https://github.com/advisories/GHSA-v6wh-96g9-6wx3)
- **D02 PostCSS**：攻击者控制构建时的 CSS/sourceMappingURL，可能读取任意 .map 文件；修复中存在后续绕过，最终需 >=8.5.23。Google Drive 文件名/文档内容没有送入 CSS 编译流程，因此当前用户数据路径不触发；不可信源代码构建仍存在暴露面。通过 8.x override 更新到 8.5.28，覆盖 Vue 编译器与 Vite 两条依赖链。[原公告](https://github.com/advisories/GHSA-r28c-9q8g-f849)、[后续修复公告](https://github.com/advisories/GHSA-fxqj-rqcc-2cmp)
- **D03 nanoid**：非安全生成器接收负数长度或自定义生成器接收零长度时无限循环。项目未直接调用这些函数，也不将用户输入作为长度；修复锁定依赖，避免未来依赖调用暴露。3.3.12 -> 3.3.19，最低完整修复版本 3.3.18。[负长度公告](https://github.com/advisories/GHSA-28wg-ghj8-5hjv)、[零长度公告](https://github.com/advisories/GHSA-2v37-7h3g-55p8)
- **D04 Tauri**：内部通道 fetch 的 ACL 和所属 WebView 绑定不足，恶意 WebView 可猜测并读取另一 WebView 的大响应。当前只有一个本地窗口，未创建远程 WebView，标准跨 WebView 攻击前提目前不满足；框架漏洞仍应修复，不将单窗口配置当成长期缓解。受影响 2.0.0–2.11.5，修复 >=2.11.6；本次 Rust/API/CLI 最终为 2.12.1。旧 Origin confusion 公告修复于 2.11.1，原 2.11.2 已不受其影响。[最新 IPC 公告](https://github.com/tauri-apps/tauri/security/advisories/GHSA-w28w-mhc8-qvjv)、[Origin 公告](https://github.com/tauri-apps/tauri/security/advisories/GHSA-7gmj-67g7-phm9)
- **D05 rustls**：应用确实通过 reqwest 使用 rustls 访问 Google HTTPS API。需恶意/被控制的 TLS 对端满足异常握手条件，不等于可无条件绕过 Google 证书校验。升级 0.23.40 -> 0.23.45。[官方公告](https://rustsec.org/advisories/RUSTSEC-2026-0285.html)
- **D06 quinn-proto**：恶意 QUIC 对端通过大量乱序流片段耗尽内存。项目直接 reqwest 未启用 HTTP/3，Windows cargo tree 查询没有该活跃包；锁文件仍覆盖潜在平台/feature 路径，升级 0.11.14 -> 0.11.15。其他平台/feature 的具体可达性为「待确认」，没有将锁文件存在等同于运行时启用。[官方 OSV 数据](https://api.osv.dev/v1/vulns/RUSTSEC-2026-0185)
- **D07 serde_with**：KeyValueMap 序列化空内部元素会 panic。第一方代码和已检查的 tauri-utils 源码未使用 KeyValueMap；未来调用或其他上游路径可达性「待确认」。兼容升级 3.20.0 -> 3.21.0。[官方公告](https://github.com/advisories/GHSA-7gcf-g7xr-8hxj)
- **D08 anyhow**：Error::downcast_mut 不健全，可能产生内存安全问题。公告为 INFO unsound，没有已确认的官方 High/Critical 等级，本报告以 Medium 跟踪；项目未直接调用该函数，上游全部路径可达性「待确认」。补丁升级 1.0.102 -> 1.0.103。[官方 OSV 数据](https://api.osv.dev/v1/vulns/RUSTSEC-2026-0190)
- **D09 event-listener**：!Send 标签通过 StackSlot 跨线程，不健全行为；无官方 CVSS，审核评级 Medium。第一方未直接使用该库，当前 Windows 活跃图中未启用相关 Linux 后端链；升级 5.4.1 -> 5.4.2。[官方公告](https://rustsec.org/advisories/RUSTSEC-2026-0221.html)
- **D10 quick-xml**：重复属性检查为平方时间复杂度，NsReader 还存在无界命名空间内存分配，两条公告均 High，修复 >=0.41.0。由 plist -> tauri-utils 引入；应用只处理 Google JSON，没有外部 XML 上传/解析入口；构建元数据可受仓库输入影响。已检查 plist 未发现 NsReader 调用，NsReader 那条公告在该路径缺乏利用前提。第一条属性检查的完整运行路径仍为「待确认」。不能因低可达性将库漏洞标成已修复。[CPU 公告](https://rustsec.org/advisories/RUSTSEC-2026-0194.html)、[内存公告](https://rustsec.org/advisories/RUSTSEC-2026-0195.html)
- **D11 glib**：VariantStrIter 的 safe Rust 接口触发未定义行为/空指针崩溃，修复 >=0.20.0。公告为 INFO unsound，审核评级 Medium。属于 Linux GTK 依赖，当前 Windows/macOS 发布没有该运行路径；未来 Linux 支持需要重新评估。[官方公告](https://rustsec.org/advisories/RUSTSEC-2024-0429.html)
- **D12 proc-macro-error**：1.0.4 已停止维护，不是已证明可远程利用的漏洞；当前 Windows 活跃依赖树未包含它。保留上游链并跟踪移除，不直接引入不兼容替代品。[官方公告](https://rustsec.org/advisories/RUSTSEC-2024-0370.html)

## 6. 代码与凭据问题具体改动

**C01：切换账号后的旧请求/缓存污染，Medium。** 原 set_access_token 只替换 token，没有清空缓存；断开虽然清缓存，正在执行的 list 请求仍可能在断开后写回旧账号文件，扫描结果还可能再次显示在前端。利用条件是同一应用实例切换账号且旧请求未完成；可能暴露旧账号文件名/所有者/扫描结果，不会绕过 Google 服务端授权。将 token、会话代次、目录缓存、扫描缓存归入同一个 Mutex；替换/清除 token 原子清缓存并取消扫描，缓存读取/写入均验证会话代次，重复使用同一 token 也不会接受旧响应。前端用会话代次丢弃旧响应，扫描事件携带 authGeneration；连接/断开禁止重入。IPC 数据只在本次编译的前后端之间传递，没有持久化旧数据结构迁移。

**C02：Drive 查询注入和文档路径片段注入，Medium。** parent_id 原直接插入 Drive q 的单引号字符串，百分号编码只编码 URL，无法防止解码后的查询语法被改变；document_id 也直接拼入路径。利用要求能调用本地 IPC 并传入构造 ID，可改变查询或字段/路径，但仍受同一 Google token 权限限制，不是 SQL 注入或 Google 权限绕过。后端统一校验 ID 非空、<=256 字节且只包含 ASCII 字母/数字/下划线/连字符，应用生成的 ID 与 root 保持可用。

**C03：CSP 关闭，Medium 纵深防御问题。** 没有发现可直接执行的 Vue XSS sink；文件名、错误信息使用模板转义，无 v-html/eval/innerHTML。原 csp:null 会在未来脚本注入时缺少拦截。生产仅允许本地脚本、Tauri IPC 连接、禁止 iframe/object/表单提交/base 修改；保留 Vue 动态样式必需的 inline style。开发单独允许 http/ws 以支持 TAURI_DEV_HOST 和 HMR；该宽松连接策略不用于生产。

**C04：外部链接及 opener 权限过宽，Medium。** 原直接将 API webViewLink 交给 openUrl，opener:default 允许任意 HTTP/HTTPS/mailto/tel 和不需要的文件揭示命令。利用要求链接元数据被污染，或前端发生脚本执行；不是已发现远程命令执行。新增 URL 规范化与精确 Google 域名/HTTPS 检查，拒绝凭据、非默认端口、控制字符和反斜杠。Tauri ACL 同时仅允许 https://drive.google.com/*、https://docs.google.com/*，移除不需要的揭示文件能力；前端绕过验证时仍受 Rust 插件 ACL 限制。

**C05：扫描取消不终止后台任务/递归无环检测，Medium。** 原每个文档都 spawn 一个任务，取消只退出接收循环，后台任务仍持有 token、继续发请求；大量目录会产生大量排队任务。改用 JoinSet，最多保留 6 个扫描任务，100ms 检查取消，退出/错误/注销时丢弃 JoinSet 会中止任务；目录读取也能通过 select 取消在途请求。后端与前端目录递归都增加访问集合。已发出的网络请求不能被撤回，客户端取消只阻止继续等待/发起后续任务。保留原并发数与重试策略。

**C06：发布标签 PowerShell 脚本注入，High。** 原 github.ref_name 被工作流表达式直接插入双引号 PowerShell 源码，包含 PowerShell 子表达式的恶意 v* 标签可能执行额外代码，污染发布产物。利用要求攻击者具备创建/推送可触发发布标签的权限；外部普通 PR 不会触发这个工作流。现在通过环境变量传值，再限制为 v 开头的版本标签字符；变量内容不会作为脚本重新解析。

**C07：可变 Action 标签，Medium 供应链加固。** 原多个第三方 Action 使用 @vN，CodeQL Rust 使用 @stable，标签可被上游移动。全部 uses 固定为通过 GitHub 官方仓库 API 验证的 40 位提交 SHA（CodeQL init/analyze 共用同一官方提交）；保留现有 Action 大版本。checkout 禁用凭据持久化；Dependabot 新增 github-actions 每周检查。未发现 pull_request_target、来自 PR 文本的脚本拼接或不必要的发布 job 权限扩张。托管 runner、Node/Rust 通道和 macOS brew 仍会更新，不声称构建完全可复现。

**C08：无限等待、自动重定向和原始错误正文，Low 加固。** 原 Client::new 没有总请求超时，Google 错误正文全部返回前端；没有发现实际泄露的 token 或关闭 TLS 校验。统一 HTTPS-only 客户端，10 秒连接超时、60 秒总请求超时、禁止重定向，网络错误去掉请求 URL，仅返回 Google HTTP 状态，不返回原始响应正文。这样减少元数据暴露、异常目标跳转与长期占用；不是已确认的任意地址 SSRF。

**S01：敏感文件忽略与输入保护，Low。** .gitignore 原未忽略 .env 或私钥文件。新增 .env/.env.*（排除 .env.example）、pem/key/p12/pfx 忽略规则，已用 git check-ignore 核对；Git 追踪列表没有这些文件，仅有正常的 src/vite-env.d.ts。令牌输入改为 password、关闭自动完成/拼写检查并限制长度，连接成功及断开后清空前端输入。后端令牌仍仅驻留内存，没有明文磁盘保存、localStorage/sessionStorage 或日志记录。

未发现真实硬编码 API Key、OAuth token、AccessKey、Webhook、密码、私钥或带密码连接串，因此没有虚构环境变量或添加无用途的 .env.example。若以后引入配置凭据，再提供仅变量名与空值的示例。若存在此次扫描之外、已经提交或泄露过的密钥，必须在对应平台作废并重新生成；只删除文件、加入 .gitignore 或清除 Git 当前版本不够。

## 7. 开发者决定

开发者已经明确选择：**保留 glib 0.18.5 和 quick-xml 0.39.4，记录风险并跟踪上游修复**。

- 方案 A（已选择、当前建议）：保留现有 Windows/macOS 功能，跟踪 Tauri/GTK/plist 上游迁移；不新增 Linux 发布或不可信 XML 输入，在新增此类功能前重新评估。
- 方案 B：单独授权升级或修改上游依赖链，迁移 glib >=0.20.0、quick-xml >=0.41.0，进行平台回归。不能用 Cargo patch 将接口不兼容版本强行塞入现有约束。

目前没有等待决定而阻塞的安全修复，也没有轮换密钥、删功能或擅自跨主要版本升级。停止维护的 proc-macro-error 继续关注上游替换；无已确认的可利用问题要求立即删除相关功能。

## 8. 验证与剩余风险

| 检查 | 结果 |
| --- | --- |
| pnpm test | 2 个链接安全测试通过，覆盖协议、伪装域名、凭据、端口和反斜杠 |
| cargo test --locked | 4 个安全测试通过，覆盖账号切换缓存隔离、扫描失效、ID 注入和禁止明文 HTTP |
| cargo clippy --locked --all-targets -- -D warnings | 通过，无警告 |
| cargo fmt --check | 通过 |
| pnpm tauri build --no-bundle | Windows x64 优化版生产构建通过，含 vue-tsc/Vite 前端构建；未启动服务器或应用 |
| 最终 pnpm audit | 0 条漏洞公告 |
| 最终 OSV | 全部 489 个锁定外部 Cargo 条目已查询；仅三个已说明的残留包 |
| YAML/Action 固定检查 | 通过；所有 uses 为 40 位 SHA |
| git diff --check | 通过 |
| 补充产物密钥特征扫描 | 新安全模块、测试、最终 dist 和 Windows release/GDriveViewer.exe 未命中凭据特征 |

Windows 生产程序位于 src-tauri/target/release/GDriveViewer.exe；未生成/安装 MSI。构建产物由现有忽略规则排除，没有将二进制提交到版本控制。

审核不包括使用真实 Google 凭据的端到端测试、macOS 实机 WebView、MSI/DMG 安装器、GitHub 托管 CI 实跑和系统 WebView/操作系统漏洞；这些均为「待确认」，不能以本地编译通过替代。

源码中没有 SQL/NoSQL 服务、上传/解压入口、扩展 manifest、动态执行命令或远程代码加载。应用通过固定 Google HTTPS 地址执行只读 GET；未发现任意地址 SSRF、开放 CORS、CSRF 网站会话、关闭证书校验、自造加密、明文持久化凭据等适用路径。

剩余建议：

1. 跟踪上述已接受的 quick-xml/glib 及停更依赖风险；不能把“扫描器 npm 为 0”写成整体项目无漏洞。
2. 生产 CSP/ACL 在原生 WebView 的实际运行效果、Google 文件链接类型和 Google ID 上界，需要在真实账号与 macOS 上复验；本次未启动应用。权限只允许应用现有的 Google 文档/云盘链接。
3. 超大目录仍会积累完整文件元数据，API 响应未设置业务级总大小/文件数上限。并发和取消已控制，但设置上限会改变扫描完整性，后续应按产品规模单独设计，当前记为 Low 建议关注。
4. 令牌由用户提供，应用不能保证 token 在 Google 平台上只有只读 scope；继续使用短期只读 token。未来 OAuth/系统凭据库方案需单独设计，当前没有新增凭据持久化。
5. 源码扫描和模式扫描不能证明所有未知凭据格式或系统内存均无秘密；本次没有读取用户系统凭据、云平台账户或不可达/已删除的远端 Git 历史。
6. 所有 Action 已固定并交由 Dependabot 跟踪；官方 actions/attest-build-provenance 建议新实现迁移到 actions/attest，现有实现可继续使用，本次不擅自迁移工作流功能。[官方说明](https://github.com/actions/attest-build-provenance)

建议 commit message：

```text
fix(security): patch vulnerable dependencies and harden sessions, IPC, and release workflows
```
