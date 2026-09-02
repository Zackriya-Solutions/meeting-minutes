from pathlib import Path
from playwright.sync_api import sync_playwright


ROOT = Path(__file__).resolve().parents[1]
URL = "http://localhost:8000/pulsetalq-identity.html"
SCREENSHOT = ROOT / "docs" / "pulsetalq-identity-preview.png"


with sync_playwright() as playwright:
    browser = playwright.chromium.launch(headless=True)
    page = browser.new_page(viewport={"width": 1440, "height": 1000}, device_scale_factor=1)
    errors = []
    page.on("console", lambda message: errors.append(message.text) if message.type == "error" else None)
    page.goto(URL, wait_until="networkidle")
    page.screenshot(path=str(SCREENSHOT), full_page=True)
    desktop = page.evaluate(
        """
        () => ({
          title: document.title,
          sections: document.querySelectorAll('.identity-section').length,
          bodyWidth: document.body.scrollWidth,
          viewportWidth: window.innerWidth,
          fontsReady: document.fonts.status,
        })
        """
    )
    desktop["consoleErrors"] = errors
    print({"desktop": desktop})

    page.set_viewport_size({"width": 390, "height": 844})
    page.reload(wait_until="networkidle")
    mobile = page.evaluate(
        """
        () => ({
          bodyWidth: document.body.scrollWidth,
          viewportWidth: window.innerWidth,
          clippedElements: [...document.querySelectorAll('body *')]
            .filter((el) => el.getBoundingClientRect().right > window.innerWidth + 1)
            .map((el) => ({tag: el.tagName, className: el.className, text: el.textContent?.trim().slice(0, 40)})),
        })
        """
    )
    print({"mobile": mobile})
    browser.close()
