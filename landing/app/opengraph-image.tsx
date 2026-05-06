import { ImageResponse } from 'next/og'

export const runtime = 'edge'
export const alt = 'Xero — The agentic coding studio for your desktop'
export const size = { width: 1200, height: 630 }
export const contentType = 'image/png'

const GOLD = '#d4a574'
const DARK = '#4e4337'
const BG = '#0b0d10'
const FG = '#f8f9fa'
const MUTED = '#a8aeb5'

function LogoMark({ size: s = 220 }: { size?: number }) {
  // Recreates the 4-quadrant Xero brand mark from icon-logo.svg.
  // Diagonal: TL + BR are gold, TR + BL are dark brown.
  const tile = (s - s * 0.13) / 2 // leave a cross-shaped gutter
  const gap = s - tile * 2
  const cell = (color: string, radius: string) => (
    <div
      style={{
        width: tile,
        height: tile,
        background: color,
        borderRadius: radius,
      }}
    />
  )
  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        width: s,
        height: s,
        gap,
      }}
    >
      <div style={{ display: 'flex', gap, height: tile }}>
        {cell(GOLD, `${tile * 0.18}px 0 0 0`)}
        {cell(DARK, `0 ${tile * 0.18}px 0 0`)}
      </div>
      <div style={{ display: 'flex', gap, height: tile }}>
        {cell(DARK, `0 0 0 ${tile * 0.18}px`)}
        {cell(GOLD, `0 0 ${tile * 0.18}px 0`)}
      </div>
    </div>
  )
}

export default async function OGImage() {
  return new ImageResponse(
    (
      <div
        style={{
          width: '100%',
          height: '100%',
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          padding: '80px 96px',
          background: BG,
          backgroundImage: `radial-gradient(ellipse 900px 500px at 50% 38%, rgba(212,165,116,0.18), transparent 70%), radial-gradient(circle at 50% 110%, rgba(212,165,116,0.08), transparent 60%)`,
          color: FG,
          fontFamily: 'sans-serif',
          position: 'relative',
        }}
      >
        {/* Subtle dot grid */}
        <div
          style={{
            position: 'absolute',
            inset: 0,
            display: 'flex',
            backgroundImage:
              'radial-gradient(rgba(255,255,255,0.05) 1px, transparent 1px)',
            backgroundSize: '28px 28px',
            opacity: 0.6,
            maskImage:
              'radial-gradient(ellipse at center, black 35%, transparent 75%)',
          }}
        />

        {/* Top-left wordmark */}
        <div
          style={{
            position: 'absolute',
            top: 56,
            left: 64,
            display: 'flex',
            alignItems: 'center',
            gap: 14,
            color: MUTED,
            fontSize: 22,
            letterSpacing: '0.02em',
          }}
        >
          <div
            style={{
              width: 10,
              height: 10,
              borderRadius: 999,
              background: GOLD,
              boxShadow: `0 0 16px ${GOLD}`,
            }}
          />
          xero.app
        </div>

        {/* Top-right pill */}
        <div
          style={{
            position: 'absolute',
            top: 56,
            right: 64,
            display: 'flex',
            alignItems: 'center',
            gap: 10,
            padding: '8px 16px',
            borderRadius: 999,
            border: '1px solid rgba(255,255,255,0.12)',
            background: 'rgba(255,255,255,0.03)',
            color: MUTED,
            fontSize: 18,
          }}
        >
          <div
            style={{
              width: 7,
              height: 7,
              borderRadius: 999,
              background: GOLD,
            }}
          />
          Beta
        </div>

        {/* Logo */}
        <LogoMark size={150} />

        {/* Wordmark */}
        <div
          style={{
            display: 'flex',
            marginTop: 40,
            fontSize: 132,
            fontWeight: 600,
            letterSpacing: '-0.04em',
            lineHeight: 1,
            color: FG,
          }}
        >
          Xero
        </div>

        {/* Tagline */}
        <div
          style={{
            display: 'flex',
            marginTop: 28,
            maxWidth: 880,
            textAlign: 'center',
            fontSize: 38,
            lineHeight: 1.2,
            letterSpacing: '-0.015em',
            color: FG,
          }}
        >
          The agentic coding studio for your desktop
        </div>

        {/* Subtitle */}
        <div
          style={{
            display: 'flex',
            marginTop: 18,
            maxWidth: 760,
            textAlign: 'center',
            fontSize: 22,
            lineHeight: 1.4,
            color: MUTED,
          }}
        >
          Custom agents and visual workflows that ship whole projects.
        </div>
      </div>
    ),
    { ...size },
  )
}
