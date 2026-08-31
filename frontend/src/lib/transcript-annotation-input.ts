import type { NewTranscriptAnnotation, TranscriptSegmentData } from '@/types';

export interface ClipboardImage {
  rgba: () => Promise<Uint8Array>;
  size: () => Promise<{ width: number; height: number }>;
}

export type ClipboardImageReader = () => Promise<ClipboardImage>;
export type WaylandClipboardImageReader = () => Promise<Uint8Array | null>;
export type PngEncoder = (
  rgba: Uint8Array,
  size: { width: number; height: number },
) => Promise<Uint8Array>;

function formatAnchorTime(seconds: number): string {
  const totalSeconds = Math.floor(seconds);
  const minutes = Math.floor(totalSeconds / 60);
  const remainingSeconds = totalSeconds % 60;
  return `[${minutes.toString().padStart(2, '0')}:${remainingSeconds.toString().padStart(2, '0')}]`;
}

/** Resolve the point at which a manual annotation will be inserted. */
export function resolveTranscriptAnchor(
  activeTimestamp: number | null,
  latestSegment?: Pick<TranscriptSegmentData, 'timestamp' | 'endTime'>,
) {
  const time = activeTimestamp ?? latestSegment?.endTime ?? latestSegment?.timestamp ?? 0;
  const isDefault = activeTimestamp === null;
  return {
    time,
    isDefault,
    label: `${formatAnchorTime(time)} ${isDefault ? 'latest transcript point (default)' : 'selected transcript point'}`,
  };
}

/** Encode the Tauri Image RGBA resource as a portable PNG for annotation storage. */
export const encodeRgbaAsPng: PngEncoder = async (rgba, size) => {
  if (size.width <= 0 || size.height <= 0 || rgba.length !== size.width * size.height * 4) {
    throw new Error('Clipboard image has invalid dimensions');
  }

  const canvas = document.createElement('canvas');
  canvas.width = size.width;
  canvas.height = size.height;
  const context = canvas.getContext('2d');
  if (!context) throw new Error('Canvas is unavailable for clipboard image encoding');

  context.putImageData(new ImageData(new Uint8ClampedArray(rgba), size.width, size.height), 0, 0);
  const blob = await new Promise<Blob>((resolve, reject) => {
    canvas.toBlob(result => {
      if (result) resolve(result);
      else reject(new Error('Failed to encode clipboard image as PNG'));
    }, 'image/png');
  });
  return new Uint8Array(await blob.arrayBuffer());
};

/** Turn a native clipboard failure into a message that helps diagnose platform-specific issues. */
export function formatClipboardImageError(error: unknown): string {
  const message = error && typeof error === 'object' && 'message' in error
    ? String(error.message)
    : String(error);
  return `Falha ao colar imagem: ${message}. Em Linux/Wayland, verifique se a imagem foi copiada para o clipboard e se a janela tem acesso a ele.`;
}

/** Keep image bytes available for the live preview when persistence returns only metadata. */
export function preserveLiveAnnotationImageData(
  persisted: TranscriptAnnotation,
  input: NewTranscriptAnnotation,
): TranscriptAnnotation {
  if (input.type !== 'image' || input.imageData === undefined) return persisted;
  return {
    ...persisted,
    imageData: [...input.imageData],
    imageMime: input.imageMime ?? persisted.imageMime,
  };
}

/** Convert live image bytes to a CSP-compatible data URL. */
export function imageDataToDataUrl(imageData: ArrayLike<number>, mime = 'image/png'): string {
  const bytes = Uint8Array.from(imageData);
  let binary = '';
  const chunkSize = 0x8000;
  for (let offset = 0; offset < bytes.length; offset += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + chunkSize));
  }
  return `data:${mime};base64,${btoa(binary)}`;
}

/** Read a native clipboard image and turn it into the existing annotation payload. */
export async function readClipboardImageAnnotation(
  readImage: ClipboardImageReader,
  anchorTime: number,
  encodePng: PngEncoder = encodeRgbaAsPng,
  readWaylandImage?: WaylandClipboardImageReader,
): Promise<NewTranscriptAnnotation | null> {
  try {
    const image = await readImage();
    const [rgba, size] = await Promise.all([image.rgba(), image.size()]);
    const png = await encodePng(rgba, size);
    return {
      type: 'image',
      anchorTime,
      imageData: Array.from(png),
      imageMime: 'image/png',
    };
  } catch (err) {
    console.error(err);
    if (!readWaylandImage) throw err;

    try {
      const png = await readWaylandImage();
      if (!png || png.length === 0) return null;
      return {
        type: 'image',
        anchorTime,
        imageData: Array.from(png),
        imageMime: 'image/png',
      };
    } catch (fallbackError) {
      console.error(fallbackError);
      throw fallbackError;
    }
  }
}
