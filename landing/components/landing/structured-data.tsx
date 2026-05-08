import { absoluteUrl, siteConfig } from "@/lib/site"

const jsonLd = {
  "@context": "https://schema.org",
  "@graph": [
    {
      "@type": "Organization",
      "@id": absoluteUrl("/#organization"),
      name: siteConfig.legalName,
      url: siteConfig.url,
      logo: absoluteUrl("/icon-logo.svg"),
      sameAs: [siteConfig.githubUrl],
    },
    {
      "@type": "WebSite",
      "@id": absoluteUrl("/#website"),
      name: siteConfig.name,
      url: siteConfig.url,
      description: siteConfig.description,
      publisher: {
        "@id": absoluteUrl("/#organization"),
      },
    },
    {
      "@type": "SoftwareApplication",
      "@id": absoluteUrl("/#software"),
      name: siteConfig.name,
      applicationCategory: "DeveloperApplication",
      operatingSystem: "macOS, Windows, Linux",
      softwareVersion: "Beta",
      description: siteConfig.description,
      url: siteConfig.url,
      codeRepository: siteConfig.githubUrl,
      publisher: {
        "@id": absoluteUrl("/#organization"),
      },
      offers: {
        "@type": "Offer",
        price: "0",
        priceCurrency: "USD",
        availability: "https://schema.org/PreOrder",
      },
    },
  ],
}

export function LandingStructuredData() {
  return (
    <script
      type="application/ld+json"
      dangerouslySetInnerHTML={{
        __html: JSON.stringify(jsonLd).replace(/</g, "\\u003c"),
      }}
    />
  )
}
