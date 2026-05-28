import { NextResponse } from "next/server"
import {
  isDownloadTarget,
  isUnsupportedDownloadTarget,
  releasePageUrl,
  resolveDownloadUrl,
  unsupportedDownloadUrls,
} from "@/lib/download-targets"

export const revalidate = 300

function redirectTo(request: Request, url: string) {
  const response = NextResponse.redirect(new URL(url, request.url), 302)
  response.headers.set("Cache-Control", "public, max-age=300, s-maxage=300")
  response.headers.set("Vary", "Sec-CH-UA-Platform, Sec-CH-UA-Arch, User-Agent")
  return response
}

export async function GET(
  request: Request,
  context: { params: Promise<{ target: string }> },
) {
  const { target } = await context.params

  if (target === "release") {
    return redirectTo(request, releasePageUrl)
  }

  if (isUnsupportedDownloadTarget(target)) {
    return redirectTo(request, unsupportedDownloadUrls[target])
  }

  if (!isDownloadTarget(target)) {
    return NextResponse.json({ error: "Unknown download target" }, { status: 404 })
  }

  return redirectTo(request, await resolveDownloadUrl(target))
}
