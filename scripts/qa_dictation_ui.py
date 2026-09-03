from pathlib import Path

from playwright.sync_api import sync_playwright


ROOT = Path(__file__).resolve().parents[1]
BASE_URL = "http://127.0.0.1:3118"
ARTIFACT_DIR = ROOT / "artifacts" / "dictation-ui"


with sync_playwright() as playwright:
    browser = playwright.chromium.launch(headless=True)
    page = browser.new_page(viewport={"width": 1440, "height": 900}, device_scale_factor=1)

    page.goto(f"{BASE_URL}/settings", timeout=60_000, wait_until="domcontentloaded")
    page.get_by_role("tab", name="Dictation").click()
    page.get_by_text("Hold-to-talk activation").wait_for()
    ARTIFACT_DIR.mkdir(parents=True, exist_ok=True)
    page.screenshot(path=str(ARTIFACT_DIR / "settings-dictation.png"), full_page=True)

    page.goto(f"{BASE_URL}/dictation-overlay", timeout=60_000, wait_until="domcontentloaded")
    mic = page.locator(".dictation-voice-cursor")
    mic.wait_for()
    assert mic.evaluate("element => getComputedStyle(element).borderRadius") == "999px"
    page.screenshot(path=str(ARTIFACT_DIR / "overlay-compact.png"), omit_background=True)

    browser.close()
