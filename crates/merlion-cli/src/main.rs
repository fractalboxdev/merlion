//! `merlion` CLI (specs/integrations.md#cli). STUB: owned by the integrations workstream.

fn main() {
    let src = std::io::read_to_string(std::io::stdin()).unwrap_or_default();
    let r = merlion_render::render(&src, &merlion_render::RenderOptions::default());
    if let Some(svg) = r.svg {
        print!("{svg}");
    } else {
        std::process::exit(1);
    }
}
