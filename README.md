# GDriveViewer

一个 Google Drive 文件查看器。

## 功能

- 使用类 Finder 的分栏视图浏览 Google Drive 文件和目录。
- 支持按目录扫描 Google 文档中是否包含图片。

## 授权说明

当前版本使用临时 access token 访问 Google API。token 需要包含以下只读权限：

- `https://www.googleapis.com/auth/drive.readonly`
- `https://www.googleapis.com/auth/documents.readonly`

access token 通常有效期较短，过期后需要重新获取。
