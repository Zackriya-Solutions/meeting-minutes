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

## Qwen3-ASR 1.7B Int8 ONNX

- Meetily model identifier: `qwen3-asr-1.7b-int8`
- ONNX artifacts: [ilmina/qwen3-asr-1.7b-sherpa-onnx](https://huggingface.co/ilmina/qwen3-asr-1.7b-sherpa-onnx)
- Pinned revision: `66fb5ea2d4d1682ff8a663bf7e788913604996a0`
- Original model: [Qwen/Qwen3-ASR-1.7B](https://huggingface.co/Qwen/Qwen3-ASR-1.7B)
- Original model license: [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0)
- Runtime: [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx), Apache License 2.0

The community ONNX repository does not publish a model card or separate license metadata. Meetily pins and verifies every downloaded file, but distributors should independently review the conversion artifact provenance and terms before shipping this optional download.

Attribution: Qwen3-ASR was developed by the Qwen team. The sherpa-onnx-compatible 1.7B export is hosted by the `ilmina` Hugging Face account.
