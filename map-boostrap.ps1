```powershell
<#
.SYNOPSIS
Creates a Claude Code / AI Agent project structure.

Example:

.\New-HermesProject.ps1 -ProjectName Hermes

#>

param(
    [Parameter(Mandatory=$true)]
    [string]$ProjectName
)

$Root = Join-Path (Get-Location) $ProjectName

Write-Host ""
Write-Host "Creating project: $ProjectName"
Write-Host ""

#--------------------------------------------------
# Directories
#--------------------------------------------------

$Folders = @(
    "",
    ".claude",

    "docs",
    "docs\RFC",
    "docs\Architecture",
    "docs\Research",
    "docs\MeetingNotes",
    "docs\Diagrams",

    "specs",
    "specs\protocol",
    "specs\api",
    "specs\schemas",

    "reference",
    "reference\MCP",
    "reference\A2A",
    "reference\ACP",
    "reference\MP2P",
    "reference\Papers",

    "src",
    "examples",
    "tests",
    "tools",
    "scripts",

    ".github",
    ".github\workflows"
)

foreach ($folder in $Folders)
{
    New-Item `
        -ItemType Directory `
        -Force `
        -Path (Join-Path $Root $folder) | Out-Null
}

#--------------------------------------------------
# Files
#--------------------------------------------------

$Files = @(
    "README.md",
    "LICENSE",
    "CHANGELOG.md",
    "CONTRIBUTING.md",
    ".gitignore",
    "CLAUDE.md",
    "AGENTS.md"
)

foreach ($file in $Files)
{
    New-Item `
        -ItemType File `
        -Force `
        -Path (Join-Path $Root $file) | Out-Null
}

#--------------------------------------------------
# README
#--------------------------------------------------

@"
# $ProjectName

AI Agent Networking Project

## Goals

- Open protocol
- Peer-to-peer
- Secure
- Extensible
- Vendor neutral

"@ | Set-Content "$Root\README.md"

#--------------------------------------------------
# CLAUDE.md
#--------------------------------------------------

@"
# Claude Project Memory

## Mission

Build an open AI Agent Networking Protocol.

## Design Goals

- Open specification
- Decentralized
- Secure
- Vendor Neutral
- Offline capable
- Extensible

## Languages

- Rust
- Python
- TypeScript

## Workflow

RFC First.

Every significant change begins as an RFC.

Implementation follows approval.

## Coding Standards

- Small commits
- Comprehensive tests
- Documentation required
- Backward compatibility preferred

## Priorities

1. Agent Identity
2. Discovery
3. Messaging
4. Task Routing
5. Capability Negotiation
6. Memory Sync

"@ | Set-Content "$Root\CLAUDE.md"

#--------------------------------------------------
# AGENTS.md
#--------------------------------------------------

@"
# Agent Roles

## Architect

Designs protocol.

## Researcher

Reads papers and standards.

## Security

Threat modeling and cryptography.

## Networking

Peer-to-peer protocols.

## Rust Engineer

Core implementation.

## Python Engineer

Rapid prototyping.

## TypeScript Engineer

Developer tools.

## Technical Writer

Maintains RFCs.

## Test Engineer

Interoperability and conformance.

"@ | Set-Content "$Root\AGENTS.md"

#--------------------------------------------------
# RFC Starter
#--------------------------------------------------

@"
# RFC 0000

## Title

Project Overview

## Status

Draft

## Summary

Describe the overall goals of the project.

## Motivation

Why this protocol exists.

## Specification

(TBD)

## Security

(TBD)

## References

(TBD)

"@ | Set-Content "$Root\docs\RFC\0000-overview.md"

#--------------------------------------------------
# Git Ignore
#--------------------------------------------------

@"
.vscode/
.idea/

bin/
obj/
target/

node_modules/

.env
.env.*

*.log
*.tmp

dist/
build/

__pycache__/

"@ | Set-Content "$Root\.gitignore"

#--------------------------------------------------
# Git Init
#--------------------------------------------------

if (Get-Command git -ErrorAction SilentlyContinue)
{
    Push-Location $Root

    git init
    git add .
    git commit -m "Initial project structure"

    Pop-Location
}

Write-Host ""
Write-Host "Project successfully created."
Write-Host ""
Write-Host "Location:"
Write-Host $Root
Write-Host ""
Write-Host "Next steps:"
Write-Host "    cd $ProjectName"
Write-Host "    claude"
Write-Host "    /init"
Write-Host ""
```
