from pathlib import Path
from playwright.sync_api import sync_playwright


ROOT = Path(__file__).resolve().parents[1]
URL = "http://localhost:3118"
SCREENSHOT = ROOT / "docs" / "pulse-talq-app-preview.png"


with sync_playwright() as playwright:
    browser = playwright.chromium.launch(headless=True)
    page = browser.new_page(viewport={"width": 1440, "height": 900}, device_scale_factor=1)
    errors = []
    page.on("console", lambda message: errors.append(message.text) if message.type == "error" else None)
    page.goto(URL, wait_until="networkidle")
    page.wait_for_timeout(1200)
    page.screenshot(path=str(SCREENSHOT), full_page=True)
    print({
        "title": page.title(),
        "bodyWidth": page.evaluate("document.body.scrollWidth"),
        "viewportWidth": page.evaluate("window.innerWidth"),
        "text": page.locator("body").inner_text()[:500],
        "consoleErrors": errors[:8],
    })
    browser.close()
