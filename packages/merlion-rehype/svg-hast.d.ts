import type { Element } from "hast";

/**
 * Parses one SVG of the core's output — a diagram or a chart — into a hast `svg`
 * element, for hosts that place `renderChart` output in a hast tree. Returns null for
 * input outside the core's output grammar.
 */
export function svgToHast(svg: string): Element | null;
