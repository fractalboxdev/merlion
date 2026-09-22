import { defineCollection } from "astro:content";
import { docsLoader } from "@astrojs/starlight/loaders";
import { docsSchema } from "@astrojs/starlight/schema";
import { specFrontmatter, specId } from "./lib/specs.mjs";

// The docs collection is Starlight's glob over src/content/docs/. `reference/specs` is a
// relative symbolic link to the repository's specs/, which the glob follows. Specs carry
// no front matter: their title comes from the first `# heading` and their description
// from the first paragraph, and README.md becomes the index of its directory.
const loader = docsLoader({ generateId: specId });

const docs = defineCollection({
  loader: {
    name: "merlion-docs-loader",
    load: (context) =>
      loader.load({
        ...context,
        parseData: (props) => context.parseData({ ...props, data: specFrontmatter(props.data, props.filePath) }),
      }),
  },
  schema: docsSchema(),
});

export const collections = { docs };
