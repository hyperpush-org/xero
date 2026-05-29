import { NextResponse } from "next/server"
import {
  detectDownloadTarget,
  resolveDownloadUrl,
} from "@/lib/download-targets"

export const revalidate = 300

function redirectTo(request: Request, url: string) {
  const response = NextResponse.redirect(new URL(url, request.url), 302)
  response.headers.set("Cache-Control", "public, max-age=300, s-maxage=300")
  response.headers.set("Vary", "Sec-CH-UA-Platform, Sec-CH-UA-Arch, User-Agent")
  return response
}

export async function GET(request: Request) {
  return redirectTo(request, await resolveDownloadUrl(detectDownloadTarget(request.headers)))
}
