# Hosted Transcription Pricing Estimate

Last checked: 2026-07-05

This estimate compares hosted batch transcription options for a 30-minute meeting.
It uses a USD/ILS mid-market exchange rate of 1 USD = 2.997 ILS.

## 30-Minute Meeting Estimate

| Provider / model | USD estimate | ILS estimate |
| --- | ---: | ---: |
| OpenAI `gpt-4o-transcribe` | $0.18 | ₪0.54 |
| Gemini 2.5 Flash Batch | ~$0.038 | ₪0.11 |
| Gemini 2.5 Flash-Lite Batch | ~$0.010 | ₪0.03 |

## Assumptions

- OpenAI `gpt-4o-transcribe` is priced at $0.006 per audio minute.
- Gemini audio input is counted at 32 tokens per second.
- A 30-minute recording is 57,600 Gemini audio input tokens.
- Gemini estimates include audio input and an assumed transcript output of about 7,500 tokens.
- The estimate does not include the later summary-generation call.

## Sources

- OpenAI pricing: https://developers.openai.com/api/docs/pricing
- Gemini pricing: https://ai.google.dev/gemini-api/docs/pricing
- Gemini audio token details: https://ai.google.dev/gemini-api/docs/audio
- USD/ILS exchange rate reference: https://wise.com/us/currency-converter/usd-to-ils-rate/history
