---
layout: default
title: Architecture
nav_order: 2
has_children: true
---

# Architecture

This section covers the internal design of Allux — how the components are organized, how they communicate, and why each design decision was made.

The most important component to understand first is the **Context Manager**, as it drives the vast majority of Allux's effectiveness with local models.

For the multi-agent orchestration design (a future session mode that drives a single local model through many short, context-isolated calls), see [Orchestra Mode](orchestra/README.md).
