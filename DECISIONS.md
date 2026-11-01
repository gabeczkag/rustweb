# Decisions

## 2026-11-01 — Deployment kit

- Choice: multi-stage `Dockerfile`, `compose.yaml` with `rustweb`, `tcps`, and
  `uc`, a `Makefile`, and a `/deploy` page with copyable commands.
- Why: removes the "which runtime, which proxy, which service" setup work and
  gives one command for the site plus optional services.
- Rejected: separate Dockerfiles per service (more moving parts), hosted SaaS
  deployment (vendor lock-in, recurring cost).

## 2026-11-01 — Marketing surface

- Choice: homepage suite cards, factual comparison table, benchmark numbers,
  quickstart, `/deploy`, SEO metadata, sitemap, robots, favicon, and security
  notes.
- Why: the differentiator is the whole suite, not another router benchmark;
  every claim links to a reproducible command or a file in the repo.
- Rejected: invented customers, fake logos, and unverifiable "fastest" claims.

## 2026-11-01 — Nested code whitespace

- Choice: propagate `preserve_whitespace` from `<pre>`/`<textarea>` into nested
  elements.
- Why: `<pre><code>...</code></pre>` is the normal code block shape; commands
  must stay line-separated and copyable.
- Rejected: normalizing newlines in JavaScript after render (the source would
  still be broken for no-JS readers and copied text).

## 2026-11-01 — Banner skill

- Choice: install the existing `banner-design` skill from the ui-ux-pro-max
  pack and allowlist it; do not create a custom replacement.
- Why: it already covers website heroes, safe zones, art direction, export, and
  accessibility. The new `/baner` and `/market` pages follow its motion rules
  (500 ms floor, pause, reduced motion).
- Rejected: a one-off custom skill with overlapping instructions.
