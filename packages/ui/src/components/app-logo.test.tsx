/** @vitest-environment jsdom */

import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { AppLogo } from "./app-logo"

describe("AppLogo", () => {
  it("uses the active primary color for all logo blocks", () => {
    const { container } = render(<AppLogo aria-label="Xero" />)
    const paths = Array.from(container.querySelectorAll("path"))

    expect(paths.map((path) => path.getAttribute("fill"))).toEqual([
      "var(--primary)",
      "var(--primary)",
      "var(--primary)",
      "var(--primary)",
    ])
    expect(paths[2]?.getAttribute("fill-opacity")).toBe("0.32")
    expect(paths[3]?.getAttribute("fill-opacity")).toBe("0.32")
  })
})
