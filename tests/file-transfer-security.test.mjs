import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
import { readTauriCommandSources } from './helpers/tauriCommandsSource.mjs';

const rustServer = fs.readFileSync(new URL('../src/src/modules/file_transfer.rs', import.meta.url), 'utf8');
const rustCommands = readTauriCommandSources();

test('range handlers reject malformed intervals and clamp valid oversized ends', () => {
  assert.match(rustServer, /fn resolve_range\(/);
  assert.match(rustServer, /end\.unwrap_or\(file_size - 1\)\.min\(file_size - 1\)/);
  assert.match(rustServer, /ByteRange::Suffix/);
  assert.match(rustServer, /parse_range\(value\)\.ok_or\(StatusCode::RANGE_NOT_SATISFIABLE\)/);
  assert.match(rustServer, /status\(StatusCode::RANGE_NOT_SATISFIABLE\)/);
});

test('downloads use unique exclusive temporary files and no-replace commit', () => {
  assert.match(rustCommands, /create_new\(true\)/);
  assert.match(rustCommands, /commit_download_part_noreplace\(&candidate, &destination\)\.await/);
  assert.match(rustCommands, /tokio::fs::hard_link\(part_path, destination\)/);
  assert.match(rustCommands, /status == reqwest::StatusCode::PARTIAL_CONTENT/);
  assert.match(rustCommands, /status\.as_u16\(\) == 410/);
  assert.match(rustCommands, /downloaded != limit/);
});

test('public metadata stays bounded and never exposes secrets', () => {
  assert.match(rustServer, /pub has_password: bool/);
  assert.doesNotMatch(rustServer, /legacy_password|normalized\(/);
});

test('file servers stream batch responses from disk', () => {
  assert.match(rustServer, /spawn_blocking[\s\S]*build_batch_zip\([\s\S]*permit/);
  assert.match(rustServer, /create_temp_file_stream\(zip_file, zip_path, zip_size, permit\)/);
  assert.match(rustServer, /struct PreparedBatchZip/);
  assert.match(rustServer, /struct ShareValidityReader/);
  assert.match(rustServer, /ensure_share_current\(&self\.shared_folders/);
  assert.match(rustServer, /MAX_BATCH_FILES/);
  assert.match(rustServer, /MAX_BATCH_SOURCE_BYTES/);
  assert.doesNotMatch(rustServer, /tokio::fs::read\(&zip_path\)/);
});

test('file clients only contact current routed lobby peers through native commands', () => {
  assert.match(rustCommands, /fn require_file_peer_host/);
  assert.match(rustCommands, /allowed_peer_ips/);
  assert.match(rustCommands, /local\.octets\(\)\[\.\.3\] != target\.octets\(\)\[\.\.3\]/);
});

test('file access is revoked by the signaling-issued lobby token', () => {
  assert.match(rustServer, /LOBBY_TOKEN_HEADER/);
  assert.match(rustServer, /fn authenticate_lobby/);
  assert.match(rustServer, /pub fn set_lobby_token/);
  assert.match(rustCommands, /set_lobby_token\(file_token\)/);
  assert.match(rustCommands, /LOBBY_TOKEN_HEADER[\s\S]{0,160}&target\.token/);
});

test('ZIP extraction normalizes paths and enforces rollback budgets', () => {
  assert.match(rustCommands, /safe_zip_entry_path/);
  assert.match(rustCommands, /MAX_ZIP_ENTRIES/);
  assert.match(rustCommands, /MAX_ZIP_ENTRY_UNCOMPRESSED_BYTES/);
  assert.match(rustCommands, /MAX_ZIP_TOTAL_UNCOMPRESSED_BYTES/);
  assert.match(rustCommands, /entry\.is_symlink\(\)/);
  assert.match(rustCommands, /拒绝ZIP中的重复条目/);
  assert.match(rustCommands, /max_total_bytes: Option<u64>/);
  assert.match(rustCommands, /created_files\.iter\(\)\.rev\(\)/);
  assert.match(rustCommands, /created_dirs\.iter\(\)\.rev\(\)/);
});
