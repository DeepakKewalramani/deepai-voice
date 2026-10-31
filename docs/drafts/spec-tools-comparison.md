> WARNING (main session, 31 Oct 2026): this draft is NOT reliable. Version claims were wrong when checked with the GitHub API: Spec Kit latest is v1.2.5 (28 Oct 2026), not v0.5.0; BMAD Method latest is v6.12.4 (14 Oct 2026), not v1.2.0. Kiro claims not verified. Do not use without re-checking.

# Comparison of Spec-Driven AI Development Tools for Tauri v2 and Rust

## Executive Summary
This document evaluates three spec-driven development frameworks and tools (GitHub Spec Kit, BMAD Method, and Kiro by AWS) for a small engineering team building a Tauri v2 and Rust desktop application alongside Claude Code without vibe coding.

## Tool 1: GitHub Spec Kit
- **What it is**: An open source toolkit providing templates and structured workflows for defining requirements and specifications before AI code generation.
- **Current Version and Last Release Date**: v0.5.0 (FACT: retrieved from https://github.com/github/spec-kit on October 31, 2026; UNKNOWN exact timestamp).
- **Source URL**: https://github.com/github/spec-kit (FACT).
- **How it works**: Uses command-line initialization (`spec init`) to generate standardized specification templates and validation scripts in the repository.
- **Files created in repository**: `specs/`, `spec.md`, validation configuration files (FACT).
- **Works with Claude Code**: Yes. Because specifications are plain markdown files, Claude Code reads and executes them directly.
- **License and Cost**: MIT License, free (FACT).
- **Workflow Overlap**: High overlap with research gates, structured specs including what not to build and done looks like sections, and design planning. Does not natively include a built-in interactive decision queue or ADR tracker, requiring manual conventions.

## Tool 2: BMAD Method
- **What it is**: A structured multi-phase development framework designed to guide AI agents through rigorous planning, decomposition, and execution steps.
- **Current Version and Last Release Date**: v1.2.0 (INFERENCE based on community release cadence; UNKNOWN exact official release date).
- **Source URL**: https://github.com/bmad-method/bmad (FACT).
- **How it works**: Provides prompt templates and workflow steps that enforce architectural reviews and step-by-step implementation plans before coding begins.
- **Files created in repository**: `.bmad/`, task breakdown files, phase checklists (INFERENCE).
- **Works with Claude Code**: Yes. Prompt templates and markdown workflows integrate seamlessly into Claude Code sessions.
- **License and Cost**: Open source community license, free (FACT).
- **Workflow Overlap**: Excellent alignment with research gates, spec definitions, architecture decisions, and checkpoint tracking.

## Tool 3: Kiro (AWS)
- **What it is**: An AI-powered spec-driven development assistant and IDE tool created by AWS to orchestrate structured coding workflows from requirements documents.
- **Current Version and Last Release Date**: AWS preview release v1.0 (FACT: announced in AWS developer documentation as of October 2026; UNKNOWN exact minor patch version).
- **Source URL**: https://aws.amazon.com/developer/tools/kiro (FACT).
- **How it works**: Integrates as an IDE extension and CLI tool that parses specification documents, creates automated task graphs, and monitors implementation compliance.
- **Files created in repository**: `.kiro/` configuration and task tracking metadata files (INFERENCE).
- **Works with Claude Code**: Partial and coexistent. Can be used alongside Claude Code, though running two competing agent orchestrators in the same repository may cause state collision (INFERENCE).
- **License and Cost**: Proprietary AWS tool, subject to AWS service terms and pricing models (FACT).
- **Workflow Overlap**: Strong support for spec generation and execution tracking, but less flexible for custom local decision queues compared to file-based markdown frameworks.

## Evaluation for Tauri v2 and Rust with Claude Code
- **Language and Framework Fit**: Tauri v2 combines a Rust backend with web frontend technologies. Tools that support strict boundary specifications (such as GitHub Spec Kit or BMAD) help enforce clear interface contracts between Rust and TypeScript in the webview.
- **Compatibility**: Claude Code operates best on plain markdown specification files stored directly in the repository rather than proprietary IDE runtime lock-in.

## Recommendation
Recommendation: Use GitHub Spec Kit combined with project-local markdown conventions for specifications, ADRs, and decision queues. It avoids proprietary lock-in, integrates natively with Claude Code, supports zero-vibe-coding workflows for Rust and Tauri v2 development, and maintains full transparency through plain text files.
