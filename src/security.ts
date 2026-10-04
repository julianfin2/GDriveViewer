export function googleDriveUrl(value: string): string {
  if (/[\u0000-\u0020\\]/.test(value)) {
    throw new Error("链接包含无效字符。");
  }
  const url = new URL(value);
  if (url.protocol !== "https:" || !["drive.google.com", "docs.google.com"].includes(url.hostname)
    || url.username || url.password || url.port) {
    throw new Error("只能打开 Google Drive 或 Google Docs 的 HTTPS 链接。");
  }
  return url.href;
}
