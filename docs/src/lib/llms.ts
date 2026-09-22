/** Sections of /llms.txt, in order, keyed by the first segment of an entry id. */
export const SECTIONS = [
  { key: "start", title: "Start" },
  { key: "guides", title: "Guides" },
  { key: "how-it-works", title: "How it works" },
  { key: "gallery", title: "Gallery" },
  { key: "reference", title: "Specs" },
] as const;

export type SectionKey = (typeof SECTIONS)[number]["key"];

/** The section of a docs entry id: `guides/theming` → `guides`; top-level pages → `start`. */
export const sectionOf = (id: string): SectionKey => {
  const first = id.split("/")[0] ?? "";
  const section = SECTIONS.find((s) => s.key === first && s.key !== "start");
  return section ? section.key : "start";
};

/** Sort key that keeps entries in section order, then by id. */
export const sectionRank = (id: string): number => SECTIONS.findIndex((s) => s.key === sectionOf(id));
