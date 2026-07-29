import { describe, expect, test } from "bun:test";
import {
  buildTelegramShareParts,
  fitToBudget,
  markdownToTelegramText,
  TELEGRAM_DRAFT_BUDGET,
} from "../../src/lib/telegramShare";

/** The Russian value shipped in translations.ts for the draft reminder line. */
const PLACEHOLDER = "<вставьте текст из буфера обмена>";

describe("markdownToTelegramText", () => {
  test("drops emphasis markers but keeps the words", () => {
    expect(markdownToTelegramText("Обсудили **сроки** и *объём* и ~~цену~~")).toBe(
      "Обсудили сроки и объём и цену",
    );
  });

  test("keeps link targets, which a bare label would lose", () => {
    expect(markdownToTelegramText("См. [дашборд](https://example.com/x?a=1&b=2)")).toBe(
      "См. дашборд (https://example.com/x?a=1&b=2)",
    );
  });

  test("leaves snake_case identifiers alone", () => {
    expect(markdownToTelegramText("Не трогаем app_settings_kv в этом релизе")).toBe(
      "Не трогаем app_settings_kv в этом релизе",
    );
  });

  test("renders bullets and checkboxes with typographic characters", () => {
    const md = ["- Первое", "* Второе", "- [x] Готово", "- [ ] Не готово"].join("\n");
    expect(markdownToTelegramText(md)).toBe(
      ["• Первое", "• Второе", "☑ Готово", "☐ Не готово"].join("\n"),
    );
  });

  test("separates sections with a blank line where headings were", () => {
    expect(markdownToTelegramText("Вступление\n## Решения\n- Раз")).toBe(
      "Вступление\n\nРешения\n• Раз",
    );
  });

  test("passes fenced code through with its indentation", () => {
    // Leading whitespace only survives inside the message; the message itself is trimmed.
    expect(markdownToTelegramText("Пример:\n```\n  a  =  1\n```")).toBe("Пример:\n  a  =  1");
  });

  test("collapses horizontal rules and runs of blank lines", () => {
    expect(markdownToTelegramText("Раз\n\n---\n\n\n\nДва")).toBe("Раз\n\nДва");
  });

  test("survives an empty or missing summary", () => {
    expect(markdownToTelegramText("")).toBe("");
    expect(markdownToTelegramText(undefined as unknown as string)).toBe("");
  });
});

describe("buildTelegramShareParts", () => {
  test("draft carries title and date, body carries the summary", () => {
    const { draft, body, full } = buildTelegramShareParts({
      title: "Планёрка",
      createdAt: "2026-07-28T10:37:00Z",
      markdown: "Обсудили запуск",
    });
    const [title, date] = draft.split("\n");
    expect(title).toBe("Планёрка");
    expect(date).not.toBe("");
    expect(body).toBe("Обсудили запуск");
    // Pasting the body after the draft must reproduce the whole message exactly.
    expect(full).toBe(`${draft}\n\n${body}`);
  });

  test("the summary body is never duplicated in the draft", () => {
    const { draft, body } = buildTelegramShareParts({
      title: "Планёрка",
      createdAt: "2026-07-28T10:37:00Z",
      markdown: "Обсудили запуск и сроки",
    });
    expect(draft).not.toContain("Обсудили");
    expect(body).not.toContain("Планёрка");
  });

  test("does not repeat a title the summary already opens with", () => {
    const { draft, body, full } = buildTelegramShareParts({
      title: "Планёрка",
      createdAt: undefined,
      markdown: "# Планёрка\n\nОбсудили запуск",
    });
    expect(draft).toBe("Планёрка");
    expect(body).toBe("Обсудили запуск");
    expect(full).toBe("Планёрка\n\nОбсудили запуск");
  });

  test("ignores an unparseable date instead of printing Invalid Date", () => {
    const { draft, full } = buildTelegramShareParts({
      title: "Планёрка",
      createdAt: "not-a-date",
      markdown: "Тело",
    });
    expect(draft).toBe("Планёрка");
    expect(full).toBe("Планёрка\n\nТело");
  });

  test("keeps the draft inside the budget even for an absurd title", () => {
    const { draft, body } = buildTelegramShareParts({
      title: "Очень длинное название встречи ".repeat(50),
      createdAt: "2026-07-28T10:37:00Z",
      markdown: "Тело",
    });
    expect(draft.length).toBeLessThanOrEqual(TELEGRAM_DRAFT_BUDGET);
    expect(body).toBe("Тело");
  });

  test("placeholder closes the draft, after the title and date", () => {
    const { draft } = buildTelegramShareParts({
      title: "Планёрка",
      createdAt: "2026-07-28T10:37:00Z",
      markdown: "Обсудили запуск",
      draftPlaceholder: PLACEHOLDER,
    });
    const lines = draft.split("\n");
    expect(lines[0]).toBe("Планёрка");
    expect(lines[1]).not.toBe("");
    expect(draft.endsWith(`\n\n${PLACEHOLDER}`)).toBe(true);
  });

  /** The placeholder is an instruction to the user, not content. */
  test("placeholder never reaches the body or the finished message", () => {
    const { body, full } = buildTelegramShareParts({
      title: "Планёрка",
      createdAt: "2026-07-28T10:37:00Z",
      markdown: "Обсудили запуск",
      draftPlaceholder: PLACEHOLDER,
    });
    expect(body).not.toContain(PLACEHOLDER);
    expect(full).not.toContain(PLACEHOLDER);
    expect(full).toBe(`Планёрка\n${new Date("2026-07-28T10:37:00Z").toLocaleString(undefined, {
      year: "numeric",
      month: "long",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    })}\n\nОбсудили запуск`);
  });

  test("a runaway title cannot push the placeholder out of the draft", () => {
    const { draft } = buildTelegramShareParts({
      title: "Очень длинное название встречи ".repeat(50),
      createdAt: "2026-07-28T10:37:00Z",
      markdown: "Тело",
      draftPlaceholder: PLACEHOLDER,
    });
    expect(draft.length).toBeLessThanOrEqual(TELEGRAM_DRAFT_BUDGET);
    expect(draft.endsWith(PLACEHOLDER)).toBe(true);
  });

  test("with no title the draft is just the reminder", () => {
    const { draft, body } = buildTelegramShareParts({
      title: "",
      createdAt: undefined,
      markdown: "Обсудили запуск",
      draftPlaceholder: PLACEHOLDER,
    });
    expect(draft).toBe(PLACEHOLDER);
    expect(body).toBe("Обсудили запуск");
  });

  test("with no title the whole summary is pasted rather than lost", () => {
    const { draft, body, full } = buildTelegramShareParts({
      title: "",
      createdAt: undefined,
      markdown: "Обсудили запуск",
    });
    expect(draft).toBe("");
    expect(body).toBe("Обсудили запуск");
    expect(full).toBe("Обсудили запуск");
  });
});

describe("fitToBudget", () => {
  test("leaves text that already fits untouched, note and all", () => {
    expect(fitToBudget("короткий текст", 100, "…")).toEqual({
      text: "короткий текст",
      truncated: false,
    });
  });

  test("cuts on a paragraph boundary and appends the note", () => {
    const text = ["Первый абзац.", "Второй абзац.", "Третий абзац."].join("\n\n");
    // room = 34 - 1 = 33 chars; the last paragraph break inside that is at 28.
    const { text: fitted, truncated } = fitToBudget(text, 34, "…");
    expect(truncated).toBe(true);
    expect(fitted).toBe("Первый абзац.\n\nВторой абзац.…");
    expect(fitted.length).toBeLessThanOrEqual(34);
  });

  test("never exceeds the budget, note included", () => {
    const note = "\n\n— полный текст в файле";
    const long = Array.from({ length: 500 }, (_, i) => `Абзац ${i}.`).join("\n\n");
    const { text } = fitToBudget(long, TELEGRAM_DRAFT_BUDGET, note);
    expect(text.length).toBeLessThanOrEqual(TELEGRAM_DRAFT_BUDGET);
    expect(text.endsWith(note)).toBe(true);
  });

  test("falls back to a hard cut when no boundary is near the end", () => {
    const { text, truncated } = fitToBudget("a".repeat(100), 10, "");
    expect(truncated).toBe(true);
    expect(text).toBe("a".repeat(10));
  });
});
