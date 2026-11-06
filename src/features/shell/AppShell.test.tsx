import { StrictMode } from "react"
import { cleanup, render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import App from "@/App"

function setHash(hash: string) {
  window.history.replaceState(null, "", `/${hash}`)
}

describe("AppShell navigation", () => {
  beforeEach(() => {
    setHash("")
  })

  afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
  })

  it("selects a direct hash route and normalizes unknown routes to the workbench", async () => {
    setHash("#knowledge")
    const view = render(<App />)

    expect(
      screen.getByRole("heading", { level: 1, name: "知识库" }),
    ).toBeInTheDocument()

    view.unmount()
    setHash("#not-a-route")
    render(<App />)

    expect(
      screen.getByRole("heading", { level: 1, name: "实时工作台" }),
    ).toBeInTheDocument()
    await waitFor(() => expect(window.location.hash).toBe("#workbench"))
  })

  it("updates the destination, heading, and hash through labelled navigation", async () => {
    const user = userEvent.setup()
    render(<App />)

    const historyLink = screen.getAllByRole("link", { name: "会话历史" })[0]
    await user.click(historyLink)

    expect(window.location.hash).toBe("#history")
    expect(
      screen.getByRole("heading", { level: 1, name: "会话历史" }),
    ).toBeInTheDocument()
    expect(historyLink).toHaveAttribute("aria-current", "page")
  })

  it("responds to browser hash changes", async () => {
    render(<App />)

    window.location.hash = "#settings"
    window.dispatchEvent(new HashChangeEvent("hashchange"))

    expect(
      await screen.findByRole("heading", { level: 1, name: "设置" }),
    ).toBeInTheDocument()
  })

  it("closes the narrow navigation after selection and restores a valid focus target", async () => {
    const user = userEvent.setup()
    render(<App />)

    const menuButton = screen.getByRole("button", { name: "打开导航菜单" })
    await user.click(menuButton)
    const menu = await screen.findByRole("dialog", { name: "导航" })
    await user.click(
      within(menu).getByRole("link", { name: "设置" }),
    )

    await waitFor(() => expect(menu).not.toBeInTheDocument())
    expect(
      screen.getByRole("heading", { level: 1, name: "设置" }),
    ).toBeInTheDocument()
    expect(window.location.hash).toBe("#settings")
    expect(menuButton).toHaveFocus()
  })

  it("cleans up hash listeners under StrictMode", () => {
    const activeListeners = new Set<EventListenerOrEventListenerObject>()
    const originalAdd = window.addEventListener.bind(window)
    const originalRemove = window.removeEventListener.bind(window)

    vi.spyOn(window, "addEventListener").mockImplementation(
      (type, listener, options) => {
        if (type === "hashchange") activeListeners.add(listener)
        originalAdd(type, listener, options)
      },
    )
    vi.spyOn(window, "removeEventListener").mockImplementation(
      (type, listener, options) => {
        if (type === "hashchange") activeListeners.delete(listener)
        originalRemove(type, listener, options)
      },
    )

    const view = render(
      <StrictMode>
        <App />
      </StrictMode>,
    )

    expect(activeListeners.size).toBe(1)
    view.unmount()
    expect(activeListeners.size).toBe(0)
  })
})
