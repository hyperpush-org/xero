//! `@wallet-standard/react` + Mobile Wallet Standard scaffold.
//!
//! Uses `@wallet-standard/react` for wallet discovery and
//! `@solana/wallet-standard-features` for Solana-specific hooks.

use super::{escape_ts_string, ScaffoldMeta, WalletScaffoldContext};

pub fn render(ctx: &WalletScaffoldContext) -> (Vec<(String, String)>, ScaffoldMeta) {
    let files = vec![
        ("package.json".into(), package_json(ctx)),
        ("tsconfig.json".into(), tsconfig()),
        ("vite.config.ts".into(), vite_config()),
        ("index.html".into(), index_html(ctx)),
        ("src/main.tsx".into(), main_tsx(ctx)),
        ("src/App.tsx".into(), app_tsx(ctx)),
        ("src/WalletsProvider.tsx".into(), providers_tsx(ctx)),
        ("src/ConnectPanel.tsx".into(), connect_panel_tsx()),
        ("src/app.css".into(), css()),
        ("README.md".into(), readme(ctx)),
        (".gitignore".into(), gitignore()),
    ];
    let meta = ScaffoldMeta {
        entrypoint: Some("src/main.tsx".into()),
        start_command: "pnpm dev".into(),
        api_key_env: None,
        next_steps: vec![
            "pnpm install".into(),
            "pnpm dev".into(),
            "The wallet list auto-discovers standard-compliant wallets from the injected `navigator.wallets`.".into(),
        ],
    };
    (files, meta)
}

fn package_json(ctx: &WalletScaffoldContext) -> String {
    format!(
        r#"{{
  "name": "{slug}",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {{
    "dev": "vite",
    "build": "tsc --noEmit && vite build",
    "preview": "vite preview",
    "typecheck": "tsc --noEmit"
  }},
  "dependencies": {{
    "@solana/wallet-standard-features": "^1.3.0",
    "@solana/web3.js": "^1.95.0",
    "@wallet-standard/app": "^1.1.0",
    "@wallet-standard/base": "^1.1.0",
    "@wallet-standard/react": "^1.0.2",
    "react": "^18.3.1",
    "react-dom": "^18.3.1"
  }},
  "devDependencies": {{
    "@types/react": "^18.3.3",
    "@types/react-dom": "^18.3.0",
    "@vitejs/plugin-react": "^4.3.1",
    "typescript": "^5.5.0",
    "vite": "^5.3.0"
  }}
}}
"#,
        slug = ctx.project_slug,
    )
}

fn tsconfig() -> String {
    r#"{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "esModuleInterop": true,
    "resolveJsonModule": true,
    "skipLibCheck": true,
    "isolatedModules": true
  },
  "include": ["src"]
}
"#
    .into()
}

fn vite_config() -> String {
    r#"import { defineConfig } from "vite"
import react from "@vitejs/plugin-react"

export default defineConfig({ plugins: [react()] })
"#
    .into()
}

fn index_html(ctx: &WalletScaffoldContext) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>{app_name}</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
"#,
        app_name = ctx.app_name,
    )
}

fn main_tsx(ctx: &WalletScaffoldContext) -> String {
    format!(
        r#"import React from "react"
import ReactDOM from "react-dom/client"
import {{ WalletsProvider }} from "./WalletsProvider"
import {{ App }} from "./App"
import "./app.css"

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <WalletsProvider>
      <App appName="{app_name}" />
    </WalletsProvider>
  </React.StrictMode>,
)
"#,
        app_name = escape_ts_string(&ctx.app_name),
    )
}

fn providers_tsx(ctx: &WalletScaffoldContext) -> String {
    format!(
        r#"import {{ type ReactNode }} from "react"
import {{ WalletsProvider as ReactWalletsProvider }} from "@wallet-standard/react"

export const RPC_URL = "{rpc_url}"

export function WalletsProvider({{ children }}: {{ children: ReactNode }}) {{
  return <ReactWalletsProvider>{{children}}</ReactWalletsProvider>
}}
"#,
        rpc_url = escape_ts_string(&ctx.rpc_url),
    )
}

fn connect_panel_tsx() -> String {
    r#"import { useState } from "react"
import { useWallets } from "@wallet-standard/react"
import { StandardConnect, StandardDisconnect } from "@wallet-standard/features"
import type { UiWalletAccount } from "@wallet-standard/react"

export function ConnectPanel() {
  const wallets = useWallets()
  const [connected, setConnected] = useState<UiWalletAccount | null>(null)

  const connect = async (name: string) => {
    const wallet = wallets.find((w) => w.name === name)
    if (!wallet) return
    const connect = wallet.features[StandardConnect] as {
      connect: () => Promise<{ accounts: UiWalletAccount[] }>
    } | undefined
    if (!connect) return
    const { accounts } = await connect.connect()
    if (accounts.length > 0) setConnected(accounts[0])
  }

  const disconnect = async () => {
    if (!connected) return
    for (const wallet of wallets) {
      const feature = wallet.features[StandardDisconnect] as
        | { disconnect: () => Promise<void> }
        | undefined
      if (feature) await feature.disconnect()
    }
    setConnected(null)
  }

  return (
    <section className="connect-panel">
      {connected ? (
        <div className="connected">
          <p>
            Connected: <code>{connected.address}</code>
          </p>
          <button type="button" onClick={() => void disconnect()}>
            Disconnect
          </button>
        </div>
      ) : (
        <ul className="wallet-list">
          {wallets.length === 0 ? (
            <li className="empty">
              No wallet-standard wallets detected — install Phantom, Solflare, or Backpack.
            </li>
          ) : (
            wallets.map((wallet) => (
              <li key={wallet.name}>
                <button
                  type="button"
                  className="wallet-button"
                  onClick={() => void connect(wallet.name)}
                >
                  {wallet.icon ? (
                    <img src={wallet.icon} alt="" width={24} height={24} />
                  ) : null}
                  <span>{wallet.name}</span>
                </button>
              </li>
            ))
          )}
        </ul>
      )}
    </section>
  )
}
"#
    .into()
}

fn app_tsx(_ctx: &WalletScaffoldContext) -> String {
    r#"import { ConnectPanel } from "./ConnectPanel"

interface AppProps {
  appName: string
}

export function App({ appName }: AppProps) {
  return (
    <main className="app">
      <header>
        <h1>{appName}</h1>
      </header>
      <ConnectPanel />
    </main>
  )
}
"#
    .into()
}

fn css() -> String {
    r#"body {
  margin: 0;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  background: #0a0a0a;
  color: #f5f5f5;
}
.app {
  min-height: 100vh;
  padding: 32px;
}
.app header {
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  padding-bottom: 16px;
  margin-bottom: 24px;
}
.connect-panel {
  max-width: 520px;
}
.wallet-list {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.wallet-button {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
  padding: 12px 16px;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 8px;
  color: inherit;
  cursor: pointer;
}
.wallet-button:hover {
  border-color: rgba(255, 255, 255, 0.2);
}
.connected code {
  word-break: break-all;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
}
"#
    .into()
}

fn readme(ctx: &WalletScaffoldContext) -> String {
    format!(
        r#"# {slug}

Wallet-Standard scaffold generated by Xero's Solana workbench.

## Run

```bash
pnpm install
pnpm dev
```

## Cluster

Baked in at scaffold time — **{cluster}** (RPC: `{rpc_url}`). Update
`RPC_URL` in `src/WalletsProvider.tsx` to switch clusters.

## What's included

- `@wallet-standard/react` wallets provider + hooks.
- Wallet discovery via `navigator.wallets` — every standard-compliant
  wallet shows up automatically (Phantom, Solflare, Backpack, newer
  Ledger Live, etc.).
- Explicit connect/disconnect flow with no auto-connect — switch that
  to `useEffect` once you're confident about the UX.

## Next

- Wire `signTransaction` + `signMessage` via the wallet's
  `SolanaSignAndSendTransaction` and `SolanaSignMessage` features.
- Replace the connect panel with a modal if you need a richer UX.
"#,
        slug = ctx.project_slug,
        cluster = ctx.cluster.as_str(),
        rpc_url = ctx.rpc_url,
    )
}

fn gitignore() -> String {
    "node_modules\ndist\n.DS_Store\n".into()
}
