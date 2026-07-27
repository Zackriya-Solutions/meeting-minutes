# Third-Party Model Notices

Meetily does not bundle the following model weights. They are downloaded separately at the user's request.

## Qwen3-ASR 0.6B Int8 ONNX

- Meetily model identifier: `qwen3-asr-0.6b-int8`
- ONNX artifacts: [csukuangfj2/sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25](https://huggingface.co/csukuangfj2/sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25)
- Pinned revision: `68818b2313fe77bd06f6a7c5068ff3ef59d02b8a`
- Original model: [Qwen/Qwen3-ASR-0.6B](https://huggingface.co/Qwen/Qwen3-ASR-0.6B)
- Original model license: [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0)
- Runtime: [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx), Apache License 2.0
- Conversion scripts: [Wasser1462/Qwen3-ASR-onnx](https://github.com/Wasser1462/Qwen3-ASR-onnx)

The downloaded ONNX repository states that its files were exported from Qwen3-ASR using the linked conversion scripts. It does not currently publish separate license metadata, so distributors should verify the conversion artifact terms in addition to the Apache-2.0 terms of the original model and runtime.

Attribution: Qwen3-ASR was developed by the Qwen team. The ONNX export was produced by the Qwen3-ASR-onnx contributors and packaged for sherpa-onnx by its maintainers.

## SenseVoiceSmall Int8 ONNX

- Meetily model identifier: `sense-voice-small-int8`
- ONNX artifacts: [csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17](https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17)
- Pinned revision: `2365baeacb507f821a0c8120fcee3d484dba7a07`
- Original model: [FunAudioLLM/SenseVoiceSmall](https://huggingface.co/FunAudioLLM/SenseVoiceSmall)
- Model license: [FunASR Model Open Source License Agreement 1.1](https://github.com/modelscope/FunASR/blob/main/MODEL_LICENSE_AGREEMENT)
- Runtime: [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx), Apache License 2.0

The model is downloaded only when the user requests it. Its upstream `LICENSE` reference is retained in the downloaded model directory. Meetily configures automatic language recognition and inverse text normalization for Mandarin Chinese, Cantonese, English, Japanese, and Korean.

Attribution: SenseVoice was developed by the FunAudioLLM team. The quantized ONNX artifacts were prepared and published by the sherpa-onnx maintainers.
