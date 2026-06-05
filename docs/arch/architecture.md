# Editor Architecture

## Overview
A high-performance, modal terminal text editor inspired by Helix, designed for efficiency and extensibility.

## Core Components
- **Engine (Backend):** Rust-based logic for buffer management, text manipulation, and LSP orchestration.
- **TUI (Frontend):** Ratatui for rendering terminal interface.
- **Data Model:** Rope-based buffer structure for $O(\log n)$ insertions/deletions.
- **Interaction Layer:** Modal editing state machine (Normal, Insert, Select modes).
- **Extensibility:** WASM-based plugin architecture.
- **LSP Interface:** Asynchronous message passing to/from LSP servers.
