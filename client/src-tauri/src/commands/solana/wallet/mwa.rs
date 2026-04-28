//! Mobile Wallet Adapter stub.
//!
//! MWA is an on-device protocol (phone wallet ↔ dapp). You cannot
//! authenticate against an MWA wallet from a desktop browser — the
//! phone has to be in the loop. Rather than ship a broken desktop
//! flow, this scaffold is a companion project that:
//!
//! - Sets up an Expo-style React Native test harness you can run
//!   against a dev phone.
//! - Bakes in the recommended `@solana-mobile/mobile-wallet-adapter-protocol`
//!   import path so the developer doesn't have to hunt.
//! - Provides a concrete checklist for testing on a physical device.

use super::{escape_ts_string, ScaffoldMeta, WalletScaffoldContext};

pub fn render(ctx: &WalletScaffoldContext) -> (Vec<(String, String)>, ScaffoldMeta) {
    let files = vec![
        ("package.json".into(), package_json(ctx)),
        ("tsconfig.json".into(), tsconfig()),
        ("app.json".into(), expo_app_json(ctx)),
        ("src/App.tsx".into(), app_tsx(ctx)),
        ("src/MwaClient.ts".into(), mwa_client_ts(ctx)),
        ("PHONE_TESTING_CHECKLIST.md".into(), checklist()),
        ("README.md".into(), readme(ctx)),
        (".gitignore".into(), gitignore()),
    ];
    let meta = ScaffoldMeta {
        entrypoint: Some("src/App.tsx".into()),
        start_command: "pnpm start".into(),
        api_key_env: None,
        next_steps: vec![
            "This scaffold is a Mobile Wallet Adapter companion — it runs on a phone, not in a browser.".into(),
            "Install Expo CLI (npm i -g expo-cli) and the Expo Go app on your phone.".into(),
            "pnpm install && pnpm start — scan the QR with Expo Go to load the app.".into(),
            "Open PHONE_TESTING_CHECKLIST.md and walk every item before shipping.".into(),
            "Install a MWA-capable wallet on the phone: Phantom, Solflare, Backpack, or fakewallet.".into(),
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
  "main": "./node_modules/expo/AppEntry.js",
  "scripts": {{
    "start": "expo start",
    "android": "expo start --android",
    "ios": "expo start --ios",
    "typecheck": "tsc --noEmit"
  }},
  "dependencies": {{
    "@solana-mobile/mobile-wallet-adapter-protocol": "^2.1.3",
    "@solana-mobile/mobile-wallet-adapter-protocol-web3js": "^2.1.3",
    "@solana/web3.js": "^1.95.0",
    "buffer": "^6.0.3",
    "expo": "^51.0.0",
    "expo-status-bar": "~1.12.1",
    "react": "18.2.0",
    "react-native": "0.74.5",
    "react-native-get-random-values": "^1.11.0"
  }},
  "devDependencies": {{
    "@types/react": "~18.2.79",
    "typescript": "^5.5.0"
  }}
}}
"#,
        slug = ctx.project_slug,
    )
}

fn tsconfig() -> String {
    r#"{
  "extends": "expo/tsconfig.base",
  "compilerOptions": {
    "strict": true,
    "jsx": "react-native",
    "moduleResolution": "node"
  },
  "include": ["src", "App.tsx", "app.json"]
}
"#
    .into()
}

fn expo_app_json(ctx: &WalletScaffoldContext) -> String {
    format!(
        r#"{{
  "expo": {{
    "name": "{app_name}",
    "slug": "{slug}",
    "version": "0.1.0",
    "orientation": "portrait",
    "platforms": ["android", "ios"],
    "android": {{
      "package": "xyz.cadence.{slug_underscore}"
    }},
    "ios": {{
      "bundleIdentifier": "xyz.cadence.{slug_underscore}"
    }}
  }}
}}
"#,
        app_name = ctx.app_name,
        slug = ctx.project_slug,
        slug_underscore = ctx.project_slug.replace('-', "_"),
    )
}

fn app_tsx(ctx: &WalletScaffoldContext) -> String {
    format!(
        r##"import 'react-native-get-random-values'
import {{ Buffer }} from 'buffer'
// @ts-expect-error — Node globals shim for MWA.
if (typeof global.Buffer === 'undefined') global.Buffer = Buffer

import {{ StatusBar }} from 'expo-status-bar'
import {{ useCallback, useState }} from 'react'
import {{ Alert, Button, SafeAreaView, Text, View }} from 'react-native'
import {{ connectAndAuthorize, disconnect, signMessage }} from './MwaClient'

const APP_NAME = "{app_name}"

export default function App() {{
  const [address, setAddress] = useState<string | null>(null)
  const [signature, setSignature] = useState<string | null>(null)

  const onConnect = useCallback(async () => {{
    try {{
      const pubkey = await connectAndAuthorize(APP_NAME)
      setAddress(pubkey)
    }} catch (err) {{
      Alert.alert('MWA connect failed', String(err))
    }}
  }}, [])

  const onSign = useCallback(async () => {{
    try {{
      const sig = await signMessage('hello from Cadence workbench')
      setSignature(sig)
    }} catch (err) {{
      Alert.alert('MWA sign failed', String(err))
    }}
  }}, [])

  const onDisconnect = useCallback(async () => {{
    await disconnect()
    setAddress(null)
    setSignature(null)
  }}, [])

  return (
    <SafeAreaView style={{{{ flex: 1, backgroundColor: '#0a0a0a', padding: 24 }}}}>
      <StatusBar style="light" />
      <Text style={{{{ color: 'white', fontSize: 22, fontWeight: '600', marginBottom: 16 }}}}>
        {{APP_NAME}}
      </Text>
      {{address ? (
        <View style={{{{ gap: 12 }}}}>
          <Text style={{{{ color: '#eee' }}}}>Connected: {{address}}</Text>
          <Button title="Sign 'hello'" onPress={{() => void onSign()}} />
          {{signature ? (
            <Text style={{{{ color: '#8aa', fontFamily: 'Menlo', fontSize: 12 }}}}>{{signature}}</Text>
          ) : null}}
          <Button title="Disconnect" color="#aa4444" onPress={{() => void onDisconnect()}} />
        </View>
      ) : (
        <Button title="Connect a mobile wallet" onPress={{() => void onConnect()}} />
      )}}
    </SafeAreaView>
  )
}}
"##,
        app_name = escape_ts_string(&ctx.app_name),
    )
}

fn mwa_client_ts(ctx: &WalletScaffoldContext) -> String {
    format!(
        r#"import {{ transact, type AuthorizationResult }} from '@solana-mobile/mobile-wallet-adapter-protocol-web3js'
import {{ PublicKey }} from '@solana/web3.js'

const RPC_URL = "{rpc_url}"
const CLUSTER: 'devnet' | 'testnet' | 'mainnet-beta' | 'custom' = '{cluster_token}'

let cachedAuth: AuthorizationResult | null = null

function buildIdentity(appName: string) {{
  return {{
    identity: {{
      name: appName,
      uri: 'https://cadence.example',
      icon: 'favicon.ico',
    }},
  }}
}}

export async function connectAndAuthorize(appName: string): Promise<string> {{
  const auth = await transact(async (wallet) => {{
    return wallet.authorize({{
      chain: CLUSTER === 'mainnet-beta' ? 'solana:mainnet' : `solana:${{CLUSTER}}`,
      ...buildIdentity(appName),
    }})
  }})
  cachedAuth = auth
  const pubkey = new PublicKey(auth.accounts[0].address)
  return pubkey.toBase58()
}}

export async function signMessage(message: string): Promise<string> {{
  if (!cachedAuth) throw new Error('not authorized; call connectAndAuthorize first')
  const authToken = cachedAuth.auth_token
  const payload = new TextEncoder().encode(message)
  const result = await transact(async (wallet) => {{
    await wallet.reauthorize({{
      auth_token: authToken,
      ...buildIdentity('Cadence'),
    }})
    return wallet.signMessages({{
      addresses: cachedAuth!.accounts.map((a) => a.address),
      payloads: [payload],
    }})
  }})
  return Buffer.from(result.signed_payloads[0]).toString('base64')
}}

export async function disconnect(): Promise<void> {{
  if (!cachedAuth) return
  const token = cachedAuth.auth_token
  cachedAuth = null
  await transact(async (wallet) => {{
    await wallet.deauthorize({{ auth_token: token }})
  }})
}}

export const RpcEndpoint = RPC_URL
"#,
        rpc_url = escape_ts_string(&ctx.rpc_url),
        cluster_token = match ctx.cluster {
            crate::commands::solana::cluster::ClusterKind::Localnet => "custom",
            crate::commands::solana::cluster::ClusterKind::MainnetFork => "custom",
            crate::commands::solana::cluster::ClusterKind::Devnet => "devnet",
            crate::commands::solana::cluster::ClusterKind::Mainnet => "mainnet-beta",
        },
    )
}

fn checklist() -> String {
    r#"# Mobile Wallet Adapter — phone testing checklist

Walk every item on a real phone before shipping. MWA cannot be smoke-tested from a laptop.

## Setup

- [ ] Install Expo Go on a phone running Android 12+ or iOS 16+.
- [ ] Install an MWA-capable wallet: Phantom (24+), Solflare, Backpack, or `fakewallet` for CI.
- [ ] Put the phone on the same Wi-Fi network as your dev machine.
- [ ] `pnpm start` and scan the QR in Expo Go.

## Connect flow

- [ ] Connect button opens the phone wallet.
- [ ] Wallet shows the app name + identity exactly as configured.
- [ ] After connect, the app shows the returned public key.

## Sign message

- [ ] `Sign 'hello'` opens the wallet's signing sheet.
- [ ] Signature returns as a base64 string (non-empty, >= 64 bytes when decoded).
- [ ] Rejecting the sign sheet shows a user-visible error — not a silent hang.

## Re-auth / session

- [ ] Close the wallet mid-transact; the app surfaces the error.
- [ ] Backgrounding Expo Go for 60s and returning still reauthorises cleanly.
- [ ] Disconnect clears the stored auth token — reconnecting prompts afresh.

## Mainnet pre-flight

- [ ] Switch `CLUSTER` in `src/MwaClient.ts` to `mainnet-beta`.
- [ ] Confirm the wallet's network selector matches before signing.
- [ ] Run every test against a *disposable* wallet first; never your prod keypair.
"#
    .into()
}

fn readme(ctx: &WalletScaffoldContext) -> String {
    format!(
        r#"# {slug}

Mobile Wallet Adapter companion scaffold generated by Cadence's Solana
workbench.

MWA requires a phone in the loop. This scaffold is an Expo-based
React Native project you run on a device so you can integration-test
the wallet flows.

## Run

```bash
pnpm install
pnpm start  # scan the QR with Expo Go on your phone
```

## Phone checklist

See [PHONE_TESTING_CHECKLIST.md](./PHONE_TESTING_CHECKLIST.md) for the
full walk-through.

## Cluster

Baked in at scaffold time — **{cluster}** (RPC: `{rpc_url}`).

## Why not just the browser?

MWA (Mobile Wallet Adapter) is a phone-native protocol; the wallet
lives on the same device as the dapp. You can build a browser flow
that *starts* an MWA handshake via a deep link, but you still need a
device to test against. Pair this scaffold with your browser dapp.
"#,
        slug = ctx.project_slug,
        cluster = ctx.cluster.as_str(),
        rpc_url = ctx.rpc_url,
    )
}

fn gitignore() -> String {
    "node_modules\n.expo\ndist\n.DS_Store\n".into()
}
