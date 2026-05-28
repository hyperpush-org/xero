import path from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))

/** @type {import('next').NextConfig} */
const nextConfig = {
  output: 'standalone',
  poweredByHeader: false,
  turbopack: {
    root: __dirname,
  },
  images: {
    unoptimized: true,
  },
  async headers() {
    return [
      {
        source: '/:path*',
        headers: [
          {
            key: 'X-Content-Type-Options',
            value: 'nosniff',
          },
          {
            key: 'Referrer-Policy',
            value: 'origin-when-cross-origin',
          },
          {
            key: 'X-Frame-Options',
            value: 'DENY',
          },
          {
            key: 'Permissions-Policy',
            value: 'camera=(), geolocation=(), microphone=(), ch-ua-arch=(self), ch-ua-platform=(self)',
          },
          {
            key: 'Accept-CH',
            value: 'Sec-CH-UA-Platform, Sec-CH-UA-Arch',
          },
          {
            key: 'Critical-CH',
            value: 'Sec-CH-UA-Platform, Sec-CH-UA-Arch',
          },
        ],
      },
    ]
  },
}

export default nextConfig
