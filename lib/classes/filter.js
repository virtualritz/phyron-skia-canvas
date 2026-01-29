//
// ColorFilter & ImageFilter - CanvasKit-compatible color matrix API
//

"use strict";

const { neon, REPR } = require("./neon");

const ø = Symbol.for("\uD83D\uDCE6"); // the attr containing the boxed struct
const DELETED = Symbol("deleted");

// Helper to create ColorFilter instance from boxed result
function wrapColorFilter(result) {
  if (result === null) return null;
  const instance = Object.create(ColorFilter.prototype);
  Object.defineProperty(instance, ø, {
    value: result,
    writable: false,
    enumerable: false,
  });
  instance[DELETED] = false;
  return instance;
}

class ColorFilter {
  /**
   * Create ColorFilter from 4x5 row-major matrix.
   * @param {Float32Array|ArrayLike<number>} matrix - 20 elements
   * @returns {ColorFilter}
   */
  static MakeMatrix(matrix) {
    if (matrix == null || typeof matrix.length !== "number") {
      throw new TypeError("Expected Float32Array or ArrayLike<number>");
    }
    if (matrix.length !== 20) {
      throw new RangeError(`Expected 20 matrix elements, got ${matrix.length}`);
    }
    const arr = Array.isArray(matrix) ? matrix : Array.from(matrix);
    return wrapColorFilter(neon.ColorFilter.makeMatrix(null, arr));
  }

  /**
   * Create ColorFilter that converts sRGB to linear gamma.
   * @returns {ColorFilter}
   */
  static MakeSRGBToLinearGamma() {
    return wrapColorFilter(neon.ColorFilter.makeSRGBToLinearGamma(null));
  }

  /**
   * Create ColorFilter that converts linear to sRGB gamma.
   * @returns {ColorFilter}
   */
  static MakeLinearToSRGBGamma() {
    return wrapColorFilter(neon.ColorFilter.makeLinearToSRGBGamma(null));
  }

  delete() {
    if (!this[DELETED]) {
      neon.ColorFilter.delete(this[ø]);
      this[DELETED] = true;
    }
  }

  get _deleted() {
    return this[DELETED];
  }

  [REPR]() {
    if (this[DELETED]) {
      return "ColorFilter (deleted)";
    }
    return `ColorFilter (${neon.ColorFilter.repr(this[ø])})`;
  }
}

// Helper to create ImageFilter instance from boxed result
function wrapImageFilter(result) {
  if (result === null) return null;
  const instance = Object.create(ImageFilter.prototype);
  Object.defineProperty(instance, ø, {
    value: result,
    writable: false,
    enumerable: false,
  });
  instance[DELETED] = false;
  return instance;
}

// Helper to validate and get boxed input filter
function getInputFilter(input) {
  if (input === null || input === undefined) return null;
  if (!(input instanceof ImageFilter)) {
    throw new TypeError("Expected ImageFilter or null");
  }
  if (input._deleted) {
    throw new Error("Input ImageFilter has been deleted");
  }
  return input[ø];
}

class ImageFilter {
  /**
   * Create ImageFilter from ColorFilter.
   * @param {ColorFilter} colorFilter
   * @param {ImageFilter|null} [input]
   * @returns {ImageFilter|null}
   */
  static MakeColorFilter(colorFilter, input = null) {
    if (!(colorFilter instanceof ColorFilter)) {
      throw new TypeError("Expected ColorFilter as first argument");
    }
    if (colorFilter._deleted) {
      throw new Error("ColorFilter has been deleted");
    }
    return wrapImageFilter(
      neon.ImageFilter.makeColorFilter(
        null,
        colorFilter[ø],
        getInputFilter(input),
      ),
    );
  }

  /**
   * Compose two ImageFilters (outer applied after inner).
   * @param {ImageFilter} outer
   * @param {ImageFilter} inner
   * @returns {ImageFilter|null}
   */
  static MakeCompose(outer, inner) {
    if (!(outer instanceof ImageFilter) || !(inner instanceof ImageFilter)) {
      throw new TypeError("Expected ImageFilter arguments");
    }
    if (outer._deleted) {
      throw new Error("Outer ImageFilter has been deleted");
    }
    if (inner._deleted) {
      throw new Error("Inner ImageFilter has been deleted");
    }
    return wrapImageFilter(
      neon.ImageFilter.makeCompose(null, outer[ø], inner[ø]),
    );
  }

  /**
   * Create blur ImageFilter.
   * @param {number} sigmaX - horizontal blur radius
   * @param {number} sigmaY - vertical blur radius
   * @param {string} [tileMode="decal"] - "clamp" | "repeat" | "mirror" | "decal"
   * @param {ImageFilter|null} [input]
   * @returns {ImageFilter|null}
   */
  static MakeBlur(sigmaX, sigmaY, tileMode = "decal", input = null) {
    return wrapImageFilter(
      neon.ImageFilter.makeBlur(
        null,
        sigmaX,
        sigmaY,
        tileMode,
        getInputFilter(input),
      ),
    );
  }

  /**
   * Create drop shadow ImageFilter.
   * @param {number} dx - shadow x offset
   * @param {number} dy - shadow y offset
   * @param {number} sigmaX - horizontal blur radius
   * @param {number} sigmaY - vertical blur radius
   * @param {string|number[]} color - CSS color string or [r,g,b,a] array (0-1)
   * @param {ImageFilter|null} [input]
   * @returns {ImageFilter|null}
   */
  static MakeDropShadow(dx, dy, sigmaX, sigmaY, color, input = null) {
    return wrapImageFilter(
      neon.ImageFilter.makeDropShadow(
        null,
        dx,
        dy,
        sigmaX,
        sigmaY,
        color,
        getInputFilter(input),
      ),
    );
  }

  /**
   * Create drop shadow ImageFilter (shadow only, no source).
   * @param {number} dx - shadow x offset
   * @param {number} dy - shadow y offset
   * @param {number} sigmaX - horizontal blur radius
   * @param {number} sigmaY - vertical blur radius
   * @param {string|number[]} color - CSS color string or [r,g,b,a] array (0-1)
   * @param {ImageFilter|null} [input]
   * @returns {ImageFilter|null}
   */
  static MakeDropShadowOnly(dx, dy, sigmaX, sigmaY, color, input = null) {
    return wrapImageFilter(
      neon.ImageFilter.makeDropShadowOnly(
        null,
        dx,
        dy,
        sigmaX,
        sigmaY,
        color,
        getInputFilter(input),
      ),
    );
  }

  delete() {
    if (!this[DELETED]) {
      neon.ImageFilter.delete(this[ø]);
      this[DELETED] = true;
    }
  }

  get _deleted() {
    return this[DELETED];
  }

  [REPR]() {
    if (this[DELETED]) {
      return "ImageFilter (deleted)";
    }
    return `ImageFilter (${neon.ImageFilter.repr(this[ø])})`;
  }
}

module.exports = { ColorFilter, ImageFilter };
