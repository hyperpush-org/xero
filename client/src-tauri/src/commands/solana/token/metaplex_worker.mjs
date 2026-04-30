#!/usr/bin/env node
// Metaplex NFT mint worker — driven by Xero's Solana workbench.
//
// Reads every input from XERO_* env vars so nothing sensitive ever
// lands in the argv. Emits a single sentinel line:
//
//   XERO_MINT_RESULT {"mint":"...","signature":"..."}
//
// The parent process parses the sentinel and surfaces both values to
// the UI / agent.

import fs from "node:fs"
import { createSignerFromKeypair, generateSigner, percentAmount, publicKey, signerIdentity } from "@metaplex-foundation/umi"
import { createUmi } from "@metaplex-foundation/umi-bundle-defaults"
import {
  createV1,
  fetchDigitalAsset,
  mintV1,
  mplTokenMetadata,
  TokenStandard,
  verifyCollectionV1,
} from "@metaplex-foundation/mpl-token-metadata"

function requireEnv(name) {
  const value = process.env[name]
  if (value === undefined || value === "") {
    throw new Error(`missing required env var ${name}`)
  }
  return value
}

function optionalEnv(name) {
  const value = process.env[name]
  return value === undefined || value === "" ? null : value
}

function parseStandard(raw) {
  switch (raw) {
    case "non_fungible":
      return TokenStandard.NonFungible
    case "fungible":
      return TokenStandard.Fungible
    case "programmable_non_fungible":
      return TokenStandard.ProgrammableNonFungible
    default:
      throw new Error(`unsupported standard ${raw}`)
  }
}

async function main() {
  const rpcUrl = requireEnv("XERO_RPC_URL")
  const authorityPath = requireEnv("XERO_AUTHORITY")
  const name = requireEnv("XERO_NAME")
  const symbol = requireEnv("XERO_SYMBOL")
  const metadataUri = requireEnv("XERO_METADATA_URI")
  const standard = parseStandard(requireEnv("XERO_STANDARD"))
  const sellerFeeBps = Number.parseInt(requireEnv("XERO_SELLER_FEE_BPS"), 10)
  const recipient = optionalEnv("XERO_RECIPIENT")
  const collection = optionalEnv("XERO_COLLECTION")

  if (!Number.isFinite(sellerFeeBps) || sellerFeeBps < 0 || sellerFeeBps > 10000) {
    throw new Error(`invalid XERO_SELLER_FEE_BPS: ${process.env.XERO_SELLER_FEE_BPS}`)
  }

  const keypairBytes = JSON.parse(fs.readFileSync(authorityPath, "utf8"))
  if (!Array.isArray(keypairBytes) || keypairBytes.length !== 64) {
    throw new Error(`authority keypair at ${authorityPath} is not a 64-byte JSON array`)
  }

  const umi = createUmi(rpcUrl).use(mplTokenMetadata())
  const authority = umi.eddsa.createKeypairFromSecretKey(new Uint8Array(keypairBytes))
  const signer = createSignerFromKeypair(umi, authority)
  umi.use(signerIdentity(signer))

  const mint = generateSigner(umi)
  const tokenOwner = recipient ? publicKey(recipient) : signer.publicKey

  const createIx = createV1(umi, {
    mint,
    authority: signer,
    name,
    symbol,
    uri: metadataUri,
    sellerFeeBasisPoints: percentAmount(sellerFeeBps / 100),
    tokenStandard: standard,
    ...(collection ? { collection: { key: publicKey(collection), verified: false } } : {}),
  })
  const createResponse = await createIx.sendAndConfirm(umi)
  const createSignature = Buffer.from(createResponse.signature).toString("base64")

  const mintIx = mintV1(umi, {
    mint: mint.publicKey,
    authority: signer,
    amount: standard === TokenStandard.Fungible ? 1_000_000n : 1n,
    tokenOwner,
    tokenStandard: standard,
  })
  const mintResponse = await mintIx.sendAndConfirm(umi)
  const mintSignature = Buffer.from(mintResponse.signature).toString("base64")

  if (collection) {
    const verifyIx = verifyCollectionV1(umi, {
      metadata: (await fetchDigitalAsset(umi, mint.publicKey)).metadata.publicKey,
      collectionMint: publicKey(collection),
      authority: signer,
    })
    await verifyIx.sendAndConfirm(umi)
  }

  const payload = {
    mint: mint.publicKey,
    signature: mintSignature,
    createSignature,
  }
  console.log(`XERO_MINT_RESULT ${JSON.stringify(payload)}`)
}

main().catch((err) => {
  console.error(`xero-metaplex-worker fatal: ${err?.stack ?? err}`)
  process.exitCode = 1
})
