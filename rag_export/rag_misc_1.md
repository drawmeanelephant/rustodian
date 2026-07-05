# RAG Export - Misc (Part 1)

### Path: ./crates/rustodian-desktop/src/snapshots/rustodian_desktop__markdown__tests__parse_markdown_commands.snap
```
---
source: crates/rustodian-desktop/src/markdown.rs
expression: parse_markdown(input)
---
[
    Header {
        level: 2,
        text: "Commands",
    },
    CodeFence {
        text: "cargo test",
    },
]

```

### Path: ./crates/rustodian-desktop/src/snapshots/rustodian_desktop__markdown__tests__parse_markdown_tasks.snap
```
---
source: crates/rustodian-desktop/src/markdown.rs
expression: parse_markdown(input)
---
[
    Task {
        text: "task one",
        checked: false,
    },
    Task {
        text: "task two",
        checked: true,
    },
]

```

### Path: ./crates/rustodian-desktop/ui/pipeline.slint
```
import { Button, VerticalBox, HorizontalBox, LineEdit, ScrollView, StandardListView } from "std-widgets.slint";

export struct SlintProjectCommand {
    name: string,
    cmd: string,
    args: string,
}

export struct SlintProject {
    id: string,
    git_branch: string,
    git_status: string,
    name: string,
    path: string,
    discovery_date: string,
    commands: [SlintProjectCommand],
}

export struct SlintMarkdownBlock {
    block_type: string,
    content: string,
    level: int,
    is_checked: bool,
    task_id: string,
}

export struct SlintPullRequest {
    number: int,
    title: string,
    author: string,
    branch: string,
    url: string,
    updated_at: string,
    is_draft: bool,
}

component SidebarButton inherits Rectangle {
    in property <string> text;
    in property <bool> active;
    callback clicked;

    background: active ? #334155 : transparent;
    border-radius: 8px;
    height: 44px;

    TouchArea {
        clicked => { root.clicked(); }
    }

    Text {
        text: root.text;
        color: root.active ? #ffffff : #94a3b8;
        x: 16px;
        font-weight: root.active ? 700 : 400;
        vertical-alignment: center;
    }
}

export component PipelineWindow inherits Window {
    title: "Rustodian Pipeline";
    preferred-width: 900px;
    preferred-height: 600px;
    background: #0f172a;

    in-out property <int> active-page: 0;

    in-out property <string> repo-slug;
    in-out property <string> target-project;
    in property <string> stream-logs;
    in property <bool> working;
    in property <[SlintProject]> projects;
    in-out property <int> selected-project-index: -1;
    in property <[SlintMarkdownBlock]> doc-blocks;

    // Remote PR Tracking models
    in property <[SlintPullRequest]> pull-requests;
    in-out property <string> pr-status: "";
    in-out property <bool> pr-has-error: false;

    in-out property <string> janitor-status: "";
    in-out property <string> janitor-bytes-reclaimable: "0 B";

    callback trigger-janitor-clean(string, bool); // (project_id, dry_run)

    callback trigger-ingest();
    callback trigger-agent-export();
    callback run-command(string, string);
    callback load-document(string);
    callback toggle-task(string, bool);
    callback trigger-fetch-prs(string); // (repo_slug)

    HorizontalLayout {
        // Sidebar
        Rectangle {
            width: 200px;
            background: #1e293b;
            VerticalLayout {
                padding: 16px;
                spacing: 8px;
                alignment: start;

                Text {
                    text: "RUSTODIAN";
                    font-size: 16px;
                    font-weight: 900;
                    color: #ffffff;
                }
                Rectangle { height: 16px; }

                SidebarButton {
                    text: "0: Ingest";
                    active: root.active-page == 0;
                    clicked => { root.active-page = 0; }
                }
                SidebarButton {
                    text: "1: Export";
                    active: root.active-page == 1;
                    clicked => { root.active-page = 1; }
                }
                SidebarButton {
                    text: "2: Explorer";
                    active: root.active-page == 2;
                    clicked => { root.active-page = 2; }
                }
                SidebarButton {
                    text: "3: Logs";
                    active: root.active-page == 3;
                    clicked => { root.active-page = 3; }
                }
                SidebarButton {
                    text: "4: Docs";
                    active: root.active-page == 4;
                    clicked => { root.active-page = 4; }
                }
                SidebarButton {
                    text: "5: Pull Requests";
                    active: root.active-page == 5;
                    clicked => { root.active-page = 5; }
                }
            }
        }

        // Main content area
        Rectangle {
            VerticalLayout {
                padding: 24px;

                // Page 0
                if root.active-page == 0 : VerticalBox {
                    alignment: start;
                    spacing: 16px;
                    Text { text: "Repository Ingest"; font-size: 24px; color: #fff; font-weight: 700; }
                    Text { text: "Repo Slug"; color: #cbd5e1; }
                    LineEdit { text <=> root.repo-slug; enabled: !root.working; }
                    Text { text: "Target Project"; color: #cbd5e1; }
                    LineEdit { text <=> root.target-project; enabled: !root.working; }
                    Button {
                        text: root.working ? "Working..." : "Ingest Repository";
                        enabled: !root.working;
                        clicked => { root.trigger-ingest(); }
                    }
                }

                // Page 1
                if root.active-page == 1 : VerticalBox {
                    alignment: start;
                    spacing: 16px;
                    Text { text: "Agent Export"; font-size: 24px; color: #fff; font-weight: 700; }
                    Text { text: "Target Project"; color: #cbd5e1; }
                    LineEdit { text <=> root.target-project; enabled: !root.working; }
                    Button {
                        text: "Export";
                        enabled: !root.working;
                        clicked => { root.trigger-agent-export(); }
                    }
                }

                // Page 2
                if root.active-page == 2 : HorizontalLayout {
                    spacing: 16px;

                    // Left pane: Project list
                    Rectangle {
                        width: 250px;
                        background: #1e293b;
                        border-radius: 8px;
                        ScrollView {
                            VerticalLayout {
                                padding: 8px;
                                spacing: 4px;
                                for project[idx] in root.projects: Rectangle {
                                    height: 40px;
                                    background: root.selected-project-index == idx ? #334155 : transparent;
                                    border-radius: 4px;
                                    TouchArea {
                                        clicked => { root.selected-project-index = idx; }
                                    }
                                    Text {
                                        text: project.name;
                                        color: #fff;
                                        x: 12px;
                                        vertical-alignment: center;
                                    }
                                    if project.git_branch != "No Git Repo" : Rectangle {
                                        background: #334155;
                                        border-radius: 4px;
                                        width: 10px;
                                        height: 10px;
                                        x: 220px;
                                        y: 15px;
                                        border-color: project.git_status == "Clean" ? #22c55e : #ef4444;
                                        border-width: 2px;
                                    }
                                }
                            }
                        }
                    }

                    // Right pane: Project details
                    Rectangle {
                        background: #1e293b;
                        border-radius: 8px;

                        if root.selected-project-index >= 0 && root.selected-project-index < root.projects.length : VerticalLayout {
                            padding: 16px;
                            spacing: 12px;
                            Text {
                                text: root.projects[root.selected-project-index].name;
                                font-size: 20px;
                                font-weight: 700;
                                color: #fff;
                            }
                            Text {
                                text: "Path: " + root.projects[root.selected-project-index].path;
                                color: #94a3b8;
                            }
                            Text {
                                text: "Git Branch: " + root.projects[root.selected-project-index].git_branch;
                                color: #94a3b8;
                            }
                            Text {
                                text: "Git Status: " + root.projects[root.selected-project-index].git_status;
                                color: root.projects[root.selected-project-index].git_status == "Clean" ? #22c55e : #ef4444;
                            }
                            Text {
                                text: "Discovered: " + root.projects[root.selected-project-index].discovery_date;
                                color: #94a3b8;
                            }

                            // Digital Janitor Section
                            Rectangle {
                                background: #0f172a;
                                border-radius: 6px;
                                border-color: #334155;
                                border-width: 1px;
                                height: 100px;

                                HorizontalLayout {
                                    padding: 12px;
                                    spacing: 16px;
                                    alignment: space-between;

                                    VerticalLayout {
                                        alignment: center;
                                        spacing: 4px;
                                        Text {
                                            text: "🧹 Digital Janitor";
                                            font-size: 14px;
                                            font-weight: 700;
                                            color: white;
                                        }
                                        Text {
                                            text: "Reclaimable Cruft: " + root.janitor-bytes-reclaimable;
                                            font-size: 12px;
                                            color: #f59e0b;
                                        }
                                        if root.janitor-status != "" : Text {
                                            text: root.janitor-status;
                                            font-size: 11px;
                                            color: #94a3b8;
                                        }
                                    }

                                    HorizontalLayout {
                                        spacing: 8px;
                                        alignment: center;
                                        Button {
                                            text: "Scan Cruft";
                                            enabled: !root.working;
                                            clicked => {
                                                root.trigger-janitor-clean(root.projects[root.selected-project-index].id, true);
                                            }
                                        }
                                        Button {
                                            text: "Purge Cruft";
                                            enabled: !root.working;
                                            clicked => {
                                                root.trigger-janitor-clean(root.projects[root.selected-project-index].id, false);
                                            }
                                        }
                                    }
                                }
                            }

                            Text {
                                text: "Commands:";
                                color: #cbd5e1;
                                font-weight: 700;
                            }
                            ScrollView {
                                VerticalLayout {
                                    spacing: 8px;
                                    for cmd in root.projects[root.selected-project-index].commands: Rectangle {
                                        background: #334155;
                                        border-radius: 6px;
                                        VerticalLayout {
                                            padding: 12px;
                                            spacing: 8px;
                                            Text { text: cmd.name; font-weight: 700; color: #fff; }
                                            Text { text: "cmd: " + cmd.cmd + " " + cmd.args; color: #94a3b8; font-family: "monospace"; }
                                            Button {
                                                text: "Run Trigger";
                                                enabled: !root.working;
                                                clicked => { root.run-command(root.projects[root.selected-project-index].name, cmd.name); }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if root.selected-project-index < 0 || root.selected-project-index >= root.projects.length : VerticalLayout {
                            alignment: center;
                            Text {
                                text: "Select a project to view details";
                                color: #94a3b8;
                                horizontal-alignment: center;
                            }
                        }
                    }
                }

                // Page 3
                if root.active-page == 3 : VerticalBox {
                    spacing: 16px;
                    Text { text: "Live Log Stream"; font-size: 24px; color: #fff; font-weight: 700; }
                    ScrollView {
                        Rectangle {
                            background: #000;
                            border-radius: 6px;
                            VerticalLayout {
                                padding: 12px;
                                Text {
                                    text: root.stream-logs;
                                    font-family: "monospace";
                                    color: #a3be8c;
                                    wrap: word-wrap;
                                }
                            }
                        }
                    }
                }

                // Page 4
                if root.active-page == 4 : VerticalBox {
                    spacing: 16px;
                    Text { text: "Documentation Viewer"; font-size: 24px; color: #fff; font-weight: 700; }
                    Button {
                        text: "Load Readme";
                        enabled: !root.working;
                        clicked => { root.load-document("README.md"); }
                    }
                    ScrollView {
                        VerticalLayout {
                            spacing: 8px;
                            for block in root.doc-blocks: VerticalLayout {
                                if block.block_type == "heading" : Text {
                                    text: block.content;
                                    font-size: max(12px, 24px - (block.level * 2px));
                                    font-weight: 700;
                                    color: #fff;
                                }
                                if block.block_type == "paragraph" : Text {
                                    text: block.content;
                                    color: #cbd5e1;
                                    wrap: word-wrap;
                                }
                                if block.block_type == "code" : Rectangle {
                                    background: #000;
                                    border-radius: 4px;
                                    VerticalLayout {
                                        padding: 8px;
                                        Text {
                                            text: block.content;
                                            color: #a3be8c;
                                            font-family: "monospace";
                                            wrap: word-wrap;
                                        }
                                    }
                                }
                                if block.block_type == "task" : HorizontalLayout {
                                    spacing: 8px;
                                    Rectangle {
                                        width: 16px;
                                        height: 16px;
                                        border-color: #94a3b8;
                                        border-width: 1px;
                                        border-radius: 2px;
                                        background: block.is_checked ? #3b82f6 : transparent;
                                        TouchArea {
                                            clicked => { root.toggle-task(block.task_id, !block.is_checked); }
                                        }
                                    }
                                    Text {
                                        text: block.content;
                                        color: #cbd5e1;
                                        vertical-alignment: center;
                                    }
                                }
                                if block.block_type == "bullet" : HorizontalLayout {
                                    spacing: 8px;
                                    padding-left: 12px;
                                    Text {
                                        text: "•";
                                        color: #38bdf8;
                                        font-weight: 900;
                                        vertical-alignment: center;
                                    }
                                    Text {
                                        text: block.content;
                                        color: #cbd5e1;
                                        wrap: word-wrap;
                                        vertical-alignment: center;
                                    }
                                }
                                if block.block_type == "numbered" : HorizontalLayout {
                                    spacing: 8px;
                                    padding-left: 12px;
                                    Text {
                                        text: block.content;
                                        color: #cbd5e1;
                                        wrap: word-wrap;
                                        vertical-alignment: center;
                                    }
                                }
                                if block.block_type == "separator" : Rectangle {
                                    height: 9px;
                                    VerticalLayout {
                                        padding-top: 4px;
                                        padding-bottom: 4px;
                                        Rectangle {
                                            height: 1px;
                                            background: #334155;
                                        }
                                    }
                                }
                                if block.block_type == "blank" : Rectangle {
                                    height: 8px;
                                }
                            }
                        }
                    }
                }

                // Page 5: Remote Pull Request Dashboard
                if root.active-page == 5 : VerticalBox {
                    spacing: 16px;
                    Text { text: "GitHub Pull Requests"; font-size: 24px; color: #fff; font-weight: 700; }

                    HorizontalLayout {
                        spacing: 12px;
                        alignment: start;
                        Text { text: "Repository Slug:"; color: #cbd5e1; vertical-alignment: center; }
                        LineEdit {
                            text <=> root.repo-slug;
                            placeholder-text: "owner/repo";
                            width: 300px;
                            enabled: !root.working;
                        }
                        Button {
                            text: root.working ? "Retrieving..." : "Get Open PRs";
                            enabled: !root.working && root.repo-slug != "";
                            clicked => { root.trigger-fetch-prs(root.repo-slug); }
                        }
                    }

                    if root.pr-status != "" : Text {
                        text: root.pr-status;
                        font-size: 13px;
                        color: root.pr-has-error ? #f87171 : #34d399;
                    }

                    ScrollView {
                        VerticalLayout {
                            spacing: 12px;

                            if root.pull-requests.length == 0 : VerticalLayout {
                                alignment: center;
                                padding: 32px;
                                Text {
                                    text: "No open pull requests loaded.";
                                    color: #64748b;
                                    font-size: 14px;
                                    horizontal-alignment: center;
                                }
                                Text {
                                    text: "Type a repository slug above or select a repository under Explorer to verify active PR branches.";
                                    color: #475569;
                                    font-size: 12px;
                                    horizontal-alignment: center;
                                }
                            }

                            for pr in root.pull-requests : Rectangle {
                                background: #1e293b;
                                border-radius: 8px;
                                border-color: pr.is_draft ? #475569 : #334155;
                                border-width: 1px;

                                VerticalLayout {
                                    padding: 16px;
                                    spacing: 8px;

                                    HorizontalLayout {
                                        alignment: space-between;
                                        Text {
                                            text: "#" + pr.number + " - " + pr.title;
                                            font-size: 15px;
                                            font-weight: 700;
                                            color: #ffffff;
                                        }
                                        Rectangle {
                                            background: pr.is_draft ? #475569 : #22c55e;
                                            border-radius: 4px;
                                            height: 20px;
                                            width: pr.is_draft ? 50px : 60px;
                                            Text {
                                                text: pr.is_draft ? "Draft" : "Open";
                                                color: #ffffff;
                                                font-size: 11px;
                                                font-weight: 700;
                                                horizontal-alignment: center;
                                                vertical-alignment: center;
                                            }
                                        }
                                    }

                                    HorizontalLayout {
                                        alignment: space-between;
                                        VerticalLayout {
                                            spacing: 4px;
                                            Text { text: "Author: " + pr.author; color: #94a3b8; font-size: 13px; }
                                            Text { text: "Source Branch: " + pr.branch; color: #38bdf8; font-size: 13px; font-family: "monospace"; }
                                        }
                                        Text {
                                            text: "Updated: " + pr.updated_at;
                                            color: #64748b;
                                            font-size: 12px;
                                            vertical-alignment: bottom;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

```

### Path: ./.github/CODEOWNERS
```
* @drawmeanelephant

```

### Path: ./migrations/002_add_project_logs.sql
```
CREATE TABLE IF NOT EXISTS project_logs (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    command_name TEXT NOT NULL,
    exit_code   INTEGER,
    log_text    TEXT NOT NULL DEFAULT '',
    run_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_logs_project ON project_logs(project_id);
CREATE INDEX IF NOT EXISTS idx_logs_run_at  ON project_logs(run_at DESC);

```

### Path: ./migrations/001_initial.sql
```
-- Rustodian Initial Schema
-- Applied by rustodian-storage migration runner

CREATE TABLE IF NOT EXISTS projects (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    path            TEXT NOT NULL UNIQUE,
    discovered_at   TEXT NOT NULL,
    last_scanned_at TEXT,
    metadata_json   TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS project_languages (
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    language    TEXT NOT NULL,
    confidence  TEXT NOT NULL DEFAULT 'high',
    PRIMARY KEY (project_id, language)
);

CREATE TABLE IF NOT EXISTS scans (
    id              TEXT PRIMARY KEY,
    root_path       TEXT NOT NULL,
    started_at      TEXT NOT NULL,
    completed_at    TEXT,
    projects_found  INTEGER NOT NULL DEFAULT 0,
    status          TEXT NOT NULL DEFAULT 'running'
);

CREATE INDEX IF NOT EXISTS idx_projects_path ON projects(path);
CREATE INDEX IF NOT EXISTS idx_scans_started ON scans(started_at DESC);

```
