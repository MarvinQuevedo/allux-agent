---
layout: default
title: Development Tasks
parent: Development
nav_order: 1
---

# Tareas de Desarrollo (Allux)
{: .no_toc }

<details open markdown="block">
<summary>Table of contents</summary>
{: .text-delta }
1. TOC
{:toc}
</details>

---

## Objetivo Operativo
Trabajar en cambios pequeños, verificables y acumulables para mejorar `allux` sin sobrecargar el script autónomo.

## Modelos Recomendados
- **Principal:** `qwen3.5:9b` (Equilibrio calidad/latencia)
- **Cola:** `qwen3-coder:30b`

---

## Reglas de Ejecución
1. **1 tarea por corrida** únicamente.
2. Limitar cambios a archivos listados.
3. No mezclar refactors con nuevas features.
4. Validar: compilar y ejecutar pruebas tras cada tarea.
5. Mantener respuestas orientadas a diff.

---

## Task Queue (T01–T10)

### T01: Verificar Baseline
Confirmar que Ollama responde, modelos instalados y el proyecto compila.

### T02: Documentar CLI de Pruebas
Guía rápida para usar `allux-cli.ts` (list/show/run/ask).

### T03: Ejecutar por ID
Implementar carga de tareas desde markdown y ejecución por ID.

### T04: Guardar Trazas
Persistir prompts, respuestas y métricas en archivos markdown con timestamp.

### T05: Modo Ask Directo
Validar envío de prompts libres con persistencia de evidencia.

### T06: Smoke Tests
Checklist de pruebas mínimas para el CLI (casos felices y errores).

### T07: Integración de Workflow
Definir uso conjunto de REPL Rust (interactivo) y CLI TS (automatizado).

### T08: Control de Alcance
Endurecer reglas para evitar que el agente se desvíe del objetivo.

### T09: Preparar Commit
Resumir cambios y redactar mensaje de commit alineado a la intención.

### T10: Plan de Continuación
Definir las siguientes 5 mejoras prioritarias.
