# Raw HTML Policy

Status: accepted for production readiness on 2026-05-02.

Sovereign does not render markdown raw HTML as executable UI. Raw HTML blocks
and inline HTML are preserved as literal markdown source text in both the
editable `SovereignEditor` pipeline and the read-only `SovereignMarkdownView`
surface.

This means:

- HTML tags remain visible text.
- Script, style, iframe, and event-handler attributes are not interpreted.
- Links and images are resolved only from markdown link/image syntax, not from
  raw HTML attributes.
- The package does not expose a sanitizer because it does not produce HTML or
  embed raw HTML into a web view.

If a consuming app needs rendered HTML, that should be an explicit app-level
feature outside Sovereign's live editor surface. That renderer must sanitize
untrusted HTML before display and must not reuse Sovereign's text-only preview
as evidence of HTML safety.
