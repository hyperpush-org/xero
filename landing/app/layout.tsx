import type { Metadata, Viewport } from 'next'
import { Geist, Geist_Mono } from 'next/font/google'
import { Analytics } from '@vercel/analytics/next'
import './globals.css'

const geistSans = Geist({
  subsets: ['latin'],
  variable: '--font-geist-sans',
  display: 'swap',
})
const geistMono = Geist_Mono({
  subsets: ['latin'],
  variable: '--font-geist-mono',
  display: 'swap',
})

const SITE_URL = process.env.NEXT_PUBLIC_SITE_URL ?? 'https://xero.app'
const TITLE = 'Xero — The agentic coding studio for your desktop'
const DESCRIPTION =
  'A native desktop app that builds production software end-to-end. Written from the ground up in Rust — agentic workflow, harness, and persistence — with Discord and Telegram pings when you are needed. Works with your existing Claude, ChatGPT, Copilot, and OpenRouter subscriptions.'

export const metadata: Metadata = {
  metadataBase: new URL(SITE_URL),
  title: TITLE,
  description: DESCRIPTION,
  generator: 'v0.app',
  icons: {
    icon: '/icon-logo.svg',
  },
  openGraph: {
    type: 'website',
    url: SITE_URL,
    siteName: 'Xero',
    title: TITLE,
    description: DESCRIPTION,
  },
  twitter: {
    card: 'summary_large_image',
    title: TITLE,
    description: DESCRIPTION,
  },
}

export const viewport: Viewport = {
  themeColor: '#10161a',
  width: 'device-width',
  initialScale: 1,
}

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode
}>) {
  return (
    <html
      lang="en"
      className={`${geistSans.variable} ${geistMono.variable} bg-background`}
    >
      <body className="font-sans antialiased">
        {children}
        {process.env.NODE_ENV === 'production' && <Analytics />}
      </body>
    </html>
  )
}
