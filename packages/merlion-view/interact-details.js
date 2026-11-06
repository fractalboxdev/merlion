// Authored Mermaid click tooltips form a hidden detail layer (specs/interaction.md).
// This module loads only for drawings carrying data-merlion-description.
export const fillDetails = (pop, node, name, description, closeDetails) => {
  pop.ariaHidden = "false";
  pop.setAttribute("role", "region");
  pop.setAttribute("aria-label", `${name} details`);
  pop.style.pointerEvents = "auto";
  const body = pop.appendChild(document.createElement("div"));
  body.textContent = description;
  body.style.cssText = "white-space:pre-line;margin:8px 0;line-height:1.6";
  const source = node.closest("a");
  const href = source?.getAttribute("href");
  if (href) {
    let url;
    try { url = new URL(href, document.baseURI); } catch {}
    if (url && ["http:", "https:", "mailto:"].includes(url.protocol)) {
      const link = pop.appendChild(document.createElement("a"));
      link.textContent = "Read more";
      link.href = url.href;
      link.style.cssText = "color:var(--merlion-accent,#0969da);display:inline-block;margin-right:16px";
      if (source.getAttribute("target") === "_blank") link.target = "_blank", link.rel = "noopener noreferrer";
    }
  }
  const close = pop.appendChild(document.createElement("button"));
  close.type = "button";
  close.textContent = "Close details";
  close.style.cssText = "display:inline-block;width:auto;height:auto;white-space:nowrap;font:inherit;line-height:1.4;color:inherit;background:none;border:1px solid #8886;border-radius:4px;padding:2px 6px;cursor:pointer";
  close.addEventListener("click", closeDetails);
};
