/**
 * Minimal PNG decoder for the parity gate: 8-bit RGB or RGBA, non-interlaced, which is
 * what rsvg-convert and Chromium screenshots write. Output is always RGBA.
 */
import { inflateSync } from "node:zlib";

export interface Raster {
  readonly width: number;
  readonly height: number;
  /** Row-major RGBA, 4 bytes per pixel, not premultiplied. */
  readonly rgba: Uint8Array;
}

const SIGNATURE = [137, 80, 78, 71, 13, 10, 26, 10];

export const decodePng = (file: Uint8Array): Raster => {
  if (file.length < 8 || SIGNATURE.some((b, i) => file[i] !== b)) throw new Error("not a PNG");
  const view = new DataView(file.buffer, file.byteOffset, file.byteLength);
  let width = 0;
  let height = 0;
  let colourType = -1;
  const idat: Uint8Array[] = [];
  for (let o = 8; o + 8 <= file.length; ) {
    const len = view.getUint32(o);
    const type = String.fromCharCode(...file.subarray(o + 4, o + 8));
    const data = file.subarray(o + 8, o + 8 + len);
    if (type === "IHDR") {
      width = view.getUint32(o + 8);
      height = view.getUint32(o + 12);
      const depth = data[8];
      colourType = data[9] ?? -1;
      if (depth !== 8 || (colourType !== 2 && colourType !== 6) || data[12] !== 0) {
        throw new Error(`unsupported PNG: depth ${depth}, colour type ${colourType}, interlace ${data[12]}`);
      }
    } else if (type === "IDAT") idat.push(data);
    else if (type === "IEND") break;
    o += 12 + len;
  }
  const bpp = colourType === 6 ? 4 : 3;
  const stride = width * bpp;
  const raw = inflateSync(Buffer.concat(idat));
  if (raw.length < height * (stride + 1)) throw new Error("truncated PNG data");
  const px = new Uint8Array(height * stride);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    const src = y * (stride + 1) + 1;
    const row = y * stride;
    for (let i = 0; i < stride; i++) {
      const x = raw[src + i] ?? 0;
      const a = i >= bpp ? (px[row + i - bpp] ?? 0) : 0;
      const b = y > 0 ? (px[row - stride + i] ?? 0) : 0;
      const c = i >= bpp && y > 0 ? (px[row - stride + i - bpp] ?? 0) : 0;
      let pred = 0;
      switch (filter) {
        case 0:
          break;
        case 1:
          pred = a;
          break;
        case 2:
          pred = b;
          break;
        case 3:
          pred = (a + b) >> 1;
          break;
        case 4: {
          const p = a + b - c;
          const pa = Math.abs(p - a);
          const pb = Math.abs(p - b);
          const pc = Math.abs(p - c);
          pred = pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
          break;
        }
        default:
          throw new Error(`bad PNG filter ${filter}`);
      }
      px[row + i] = (x + pred) & 0xff;
    }
  }
  if (bpp === 4) return { width, height, rgba: px };
  const rgba = new Uint8Array(width * height * 4);
  for (let i = 0, j = 0; i < px.length; i += 3, j += 4) {
    rgba[j] = px[i] ?? 0;
    rgba[j + 1] = px[i + 1] ?? 0;
    rgba[j + 2] = px[i + 2] ?? 0;
    rgba[j + 3] = 255;
  }
  return { width, height, rgba };
};
