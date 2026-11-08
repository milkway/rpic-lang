---
tags: [rpic]
max_turns: 30
timeout_seconds: 900
allowed_tools: [Read, Glob, Grep, Skill, Write, Edit, Bash]
---

Faz um diagrama de arquitetura do nosso checkout: o app Next.js chama a API em Rust (axum), que usa Postgres como banco principal e Redis para sessões; a API também chama o Stripe por webhooks. Quero destacar o caminho API→Postgres. Me entrega o fonte do diagrama e um PNG.
