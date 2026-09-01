from pathlib import Path
from playwright.sync_api import sync_playwright


ROOT = Path(__file__).resolve().parents[1]
URL = "http://localhost:8000/brand-directions.html"
SCREENSHOT = ROOT / "docs" / "brand-directions-preview.png"


with sync_playwright() as playwright:
    browser = playwright.chromium.launch(headless=True)
    page = browser.new_page(viewport={"width": 1440, "height": 1000}, device_scale_factor=1)
    errors = []
    page.on("console", lambda message: errors.append(message.text) if message.type == "error" else None)
    page.goto(URL, wait_until="networkidle")
    page.screenshot(path=str(SCREENSHOT), full_page=True)

    result = page.evaluate(
        """
        () => ({
          title: document.title,
          directions: [...document.querySelectorAll('.direction')].map((el) => ({
            name: el.querySelector('h1')?.textContent,
            width: el.getBoundingClientRect().width,
            scrollWidth: el.scrollWidth,
          })),
          bodyWidth: document.body.scrollWidth,
          viewportWidth: window.innerWidth,
          fontsReady: document.fonts.status,
        })
        """
    )
    result["consoleErrors"] = errors
    print(result)

    page.set_viewport_size({"width": 390, "height": 844})
    page.reload(wait_until="networkidle")
    mobile = page.evaluate(
        """
        () => ({
          bodyWidth: document.body.scrollWidth,
          viewportWidth: window.innerWidth,
          directionWidths: [...document.querySelectorAll('.direction')]
            .map((el) => el.getBoundingClientRect().width),
        })
        """
    )
    print({"mobile": mobile})
    browser.close()
