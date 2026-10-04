import assert from "node:assert/strict";
import test from "node:test";
import { googleDriveUrl } from "../src/security.ts";

test("Google Drive links retain their path and query", () => {
  for (const url of ["https://drive.google.com/file/d/abc/view?usp=sharing", "https://docs.google.com/document/d/abc/edit"]) {
    assert.equal(googleDriveUrl(url), url);
  }
});

test("external protocols, deceptive hosts and credentials are rejected", () => {
  for (const url of [
    "http://drive.google.com/file", "javascript:alert(1)", "file:///C:/Windows/notepad.exe",
    "mailto:user@example.com", "https://drive.google.com.evil.example/file",
    "https://drive.google.com@evil.example/file", "https://evil.example@drive.google.com/file",
    "https://docs.google.com:8443/file", "https://example.com/https://docs.google.com/file",
    "//docs.google.com/file", "https://docs.google.com\\@evil.example/file",
  ]) assert.throws(() => googleDriveUrl(url), url);
});
