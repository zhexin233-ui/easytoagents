# Official Claude and Codex icon evidence

## Claude

- Official source: `https://anthropic.com/press-kit`
- Discovery evidence: the official Anthropic domain maps `/press-kit` as its media-resource download.
- Archive entry: `Anthropic media resources/Anthropic logos/Claude logos/4 Claude icon/SVG/ClaudeIcon-Square.svg`
- Local verified source used for implementation: `/tmp/easytoagents-official-icons/ClaudeIcon-Square.svg`
- SHA-256: `7f8cf3b32b0baddd3b412d235f472f0ee4081f252e3eaceb29b8398fdfe17645`
- Constraint: copy the SVG unchanged. Do not redraw or simplify its path.

## Codex

- Official product source: `https://openai.com/codex/`
- Official application source: `/Applications/ChatGPT.app`, bundle identifier `com.openai.codex`, version `26.820.60940`.
- Archive/resource entry: `/Applications/ChatGPT.app/Contents/Resources/icon-codex-light.png`
- SHA-256: `de7d43f3386105ab20952958c2c25beb0d903e2aeb6e1aef57c49a648c0d1c07`
- Constraint: copy the PNG unchanged. Do not substitute a hand-drawn OpenAI knot or generic terminal glyph.

## Rendering contract

- Package both official assets locally; do not load them from a runtime CDN.
- Render the asset itself without changing its geometry or colors.
- The assigned state uses full opacity. The unassigned state may use opacity/filter styling only to express the user-requested dimmed state.
- Preserve `aria-label`, `title`, and `aria-pressed`; the visual dimming is supplementary.

## Reference-layout interpretation

- The user-provided screenshot establishes a compact three-column card pattern, not a request to adopt its dark theme.
- Grid cards have a concise body and a separate bottom action bar divided by a border.
- Platform icons live together on the right side of the action bar.
- Existing domain actions remain available; details that make narrow cards excessively tall may remain in list mode and be summarized in grid mode.
