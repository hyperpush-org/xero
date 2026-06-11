import assert from 'node:assert/strict'
import { test } from 'vitest'

import { buildTauriDevEnv } from './tauri-dev.mjs'

test('Tauri dev env defaults to local-source launch mode', () => {
  const env = buildTauriDevEnv(
    {},
    {
      devAppDataDir: '/tmp/xero-dev-data',
      runner: '/tmp/tauri-dev-runner.sh',
      sidecarPath: '/tmp/xero-desktop-sidecar',
    },
  )

  assert.equal(env.XERO_LAUNCH_MODE, 'local-source')
  assert.equal(env.XERO_APP_DATA_DIR, '/tmp/xero-dev-data')
  assert.equal(env.XERO_DESKTOP_SIDECAR_PATH, '/tmp/xero-desktop-sidecar')
  assert.equal(env.CARGO_TARGET_AARCH64_APPLE_DARWIN_RUNNER, '/tmp/tauri-dev-runner.sh')
  assert.equal(env.CARGO_TARGET_X86_64_APPLE_DARWIN_RUNNER, '/tmp/tauri-dev-runner.sh')
})

test('Tauri dev env preserves explicit developer overrides', () => {
  const env = buildTauriDevEnv(
    {
      CARGO_BUILD_JOBS: '2',
      XERO_APP_DATA_DIR: '/custom/app-data',
      XERO_LAUNCH_MODE: 'custom-mode',
    },
    {
      devAppDataDir: '/tmp/xero-dev-data',
      runner: '/tmp/tauri-dev-runner.sh',
      sidecarPath: '/tmp/xero-desktop-sidecar',
    },
  )

  assert.equal(env.CARGO_BUILD_JOBS, '2')
  assert.equal(env.XERO_APP_DATA_DIR, '/custom/app-data')
  assert.equal(env.XERO_LAUNCH_MODE, 'custom-mode')
})
