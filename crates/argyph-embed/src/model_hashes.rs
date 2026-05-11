// SAFETY NOTE: These are the SHA-256 hashes for the FP32 ONNX model.
// BAAI does not provide an int8-quantized ONNX export on Hugging Face.
// The model is ~133 MB on disk and ~30 MB at inference time after ONNX Runtime
// loads it. Future work: evaluate dynamic quantization via ort.

pub const BGE_SMALL_ONNX_SHA256: &str =
    "828e1496d7fabb79cfa4dcd84fa38625c0d3d21da474a00f08db0f559940cf35";
pub const BGE_SMALL_TOKENIZER_SHA256: &str =
    "d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66";
