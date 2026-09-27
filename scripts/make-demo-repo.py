#!/usr/bin/env python3
"""Build the fictional "tidepool" repo used for the marketing screenshots."""
import os
import shutil
import subprocess
import sys
from datetime import datetime, timedelta, timezone

repo = sys.argv[1]
shutil.rmtree(repo, ignore_errors=True)
os.makedirs(repo)

AUTHORS = {
    "maya": ("Maya Lindqvist", "maya@example.com"),
    "tomas": ("Tomás Ferreira", "tomas@example.com"),
    "priya": ("Priya Raman", "priya@example.com"),
    "jonah": ("Jonah Okafor", "jonah@example.com"),
    "elena": ("Elena Petrova", "elena@example.com"),
    "sam": ("Sam Whitaker", "sam@example.com"),
}
clock = datetime.now(timezone.utc).replace(hour=9, minute=12, second=0, microsecond=0) - timedelta(days=25)
step = 0


def git(*args, author="maya"):
    global clock, step
    step += 1
    clock += timedelta(hours=3 + (step * 7) % 11, minutes=(step * 23) % 60)
    name, email = AUTHORS[author]
    stamp = clock.isoformat()
    env = dict(os.environ, GIT_AUTHOR_NAME=name, GIT_AUTHOR_EMAIL=email, GIT_COMMITTER_NAME=name,
               GIT_COMMITTER_EMAIL=email, GIT_AUTHOR_DATE=stamp, GIT_COMMITTER_DATE=stamp)
    subprocess.run(["git", "-C", repo, *args], check=True, env=env, stdout=subprocess.DEVNULL)


def write(path, text, mode="a"):
    full = os.path.join(repo, path)
    os.makedirs(os.path.dirname(full) or repo, exist_ok=True)
    with open(full, mode) as f:
        f.write(text)


def commit(author, message, path, text):
    write(path, text)
    git("add", "-A", author=author)
    git("commit", "-q", "-m", message, author=author)


def branch(name):
    subprocess.run(["git", "-C", repo, "switch", "-q", "-c", name], check=True)


def switch(name):
    subprocess.run(["git", "-C", repo, "switch", "-q", name], check=True)


def merge(author, name):
    switch("main")
    git("merge", "-q", "--no-ff", "-m", f"Merge branch '{name}'", name, author=author)


subprocess.run(["git", "init", "-q", "-b", "main", repo], check=True)
commit("maya", "Set up the project with Vite and TypeScript", "package.json",
       '{\n  "name": "tidepool",\n  "version": "0.1.0",\n  "type": "module"\n}\n')
commit("maya", "Add a task list backed by local storage", "src/tasks.ts",
       "export type Task = { id: string; title: string; done: boolean };\n\n"
       "export const load = (): Task[] => JSON.parse(localStorage.getItem('tasks') ?? '[]');\n")
commit("tomas", "Write setup steps in the README", "README.md",
       "# Tidepool\n\nA calm little task tracker.\n\n    npm install\n    npm run dev\n")

branch("feat/search")
commit("priya", "Add a search box to the header", "src/search.ts",
       "export const searchBox = () => document.querySelector<HTMLInputElement>('#search')!;\n")
switch("main")
commit("jonah", "Fix date parsing on Safari", "src/dates.ts",
       "export const parse = (s: string) => new Date(s.replace(' ', 'T'));\n")
switch("feat/search")
commit("priya", "Filter tasks as you type", "src/search.ts",
       "\nexport const matches = (title: string, query: string) =>\n"
       "  title.toLowerCase().includes(query.trim().toLowerCase());\n")

switch("main")
branch("fix/empty-state")
commit("elena", "Show a friendly empty state", "src/empty.ts",
       "export const emptyState = 'Nothing here yet. Add your first task.';\n")
switch("feat/search")
commit("priya", "Highlight matches in task titles", "src/search.ts",
       "\nexport const highlight = (title: string, query: string) =>\n"
       "  title.replace(new RegExp(query, 'gi'), (m) => `<mark>${m}</mark>`);\n")
merge("maya", "fix/empty-state")

branch("feat/labels")
commit("sam", "Add colored labels to tasks", "src/labels.ts",
       "export type Label = { name: string; color: string };\n")
merge("maya", "feat/search")
switch("feat/labels")
commit("sam", "Let labels be renamed from the sidebar", "src/labels.ts",
       "\nexport const rename = (label: Label, name: string): Label => ({ ...label, name });\n")
switch("main")
write("package.json", '{\n  "name": "tidepool",\n  "version": "0.2.0",\n  "type": "module",\n'
      '  "devDependencies": { "typescript": "^5.9.2", "vite": "^7.1.4" }\n}\n', mode="w")
git("commit", "-qam", "Bump dependencies", author="tomas")
commit("tomas", "Write the changelog for 0.2.0", "CHANGELOG.md",
       "## 0.2.0\n\n- Search with highlighted matches\n- Friendly empty state\n")
git("tag", "-a", "v0.2.0", "-m", "v0.2.0", author="tomas")

branch("feat/quick-switch")
commit("jonah", "Add keyboard shortcut for quick switch", "src/keys.ts",
       "export const bindings = { quickSwitch: 'Mod+K' };\n")
switch("feat/labels")
commit("sam", "Tweak sidebar spacing", "src/sidebar.css",
       ".sidebar { padding: 12px 10px; gap: 6px; }\n")
switch("main")
branch("fix/sync-folder")
commit("elena", "Fix crash when the sync folder is missing", "src/sync.ts",
       "export const syncDir = (dir?: string) => dir ?? defaultDir();\n")
merge("maya", "fix/sync-folder")
merge("maya", "feat/labels")
switch("feat/quick-switch")
commit("jonah", "Support j and k to move through results", "src/keys.ts",
       "\nexport const move = { down: 'j', up: 'k' };\n")
commit("jonah", "Show shortcut hints in menus", "src/menu.ts",
       "export const hint = (keys: string) => `<kbd>${keys}</kbd>`;\n")

switch("main")
branch("feat/dark-mode")
commit("priya", "Add dark theme tokens", "src/theme.css",
       ":root { --bg: #fbfaf7; --ink: #1d1f24; }\n")
merge("maya", "feat/quick-switch")
switch("feat/dark-mode")
commit("priya", "Follow the system appearance", "src/theme.css",
       "@media (prefers-color-scheme: dark) { :root { --bg: #16181d; --ink: #eceae4; } }\n")
switch("main")
commit("tomas", "Write the changelog for 0.3.0", "CHANGELOG.md",
       "\n## 0.3.0\n\n- Quick switch with Mod+K\n- Colored labels\n")
git("tag", "-a", "v0.3.0", "-m", "v0.3.0", author="tomas")
switch("feat/dark-mode")
commit("elena", "Fix low contrast on muted text", "src/theme.css",
       ":root { --muted: #6b7079; }\n")

switch("main")
branch("feat/export")
commit("sam", "Export tasks as Markdown", "src/export.ts",
       "export const toMarkdown = (tasks: Task[]) =>\n"
       "  tasks.map((t) => `- [${t.done ? 'x' : ' '}] ${t.title}`).join('\\n');\n")
merge("maya", "feat/dark-mode")
commit("jonah", "Lazy load icons to speed up the first render\n\n"
       "Every icon was bundled into the main chunk, adding 180 KB to the first load. "
       "Loading them on demand takes the first render on a mid-range phone from 1.4 s to 0.6 s.", "src/icons.ts",
       "export const icon = (name: string) => import(`./icons/${name}.svg`);\n")
switch("feat/export")
commit("sam", "Add CSV export", "src/export.ts",
       "\nexport const toCsv = (tasks: Task[]) => tasks.map((t) => `${t.id},${t.title}`).join('\\n');\n")

switch("main")
branch("fix/touch-drag")
commit("elena", "Fix drag and drop on touch screens", "src/drag.ts",
       "export const dragEvents = ['pointerdown', 'pointermove', 'pointerup'];\n")
switch("main")
branch("feat/reminders")
commit("priya", "Add reminders for tasks that are due", "src/reminders.ts",
       "export const dueSoon = (due: Date, now = new Date()) => due.getTime() - now.getTime() < 36e5;\n")
switch("feat/export")
commit("sam", "Include due dates in exports", "src/export.ts",
       "\nexport const dueColumn = (t: Task & { due?: string }) => t.due ?? '';\n")
switch("fix/touch-drag")
commit("elena", "Keep the dragged card under the finger", "src/drag.ts",
       "\nexport const offset = (e: PointerEvent, rect: DOMRect) => [e.clientX - rect.left, e.clientY - rect.top];\n")
switch("main")
commit("tomas", "Polish the settings page", "src/settings.css",
       ".settings { max-width: 560px; margin: 0 auto; }\n")
switch("feat/reminders")
commit("priya", "Snooze a reminder for an hour", "src/reminders.ts",
       "\nexport const snooze = (due: Date) => new Date(due.getTime() + 36e5);\n")

switch("main")
head = subprocess.check_output(["git", "-C", repo, "rev-parse", "main"], text=True).strip()
subprocess.run(["git", "-C", repo, "update-ref", "refs/remotes/origin/main", head], check=True)
for name in ["feat/reminders"]:
    sha = subprocess.check_output(["git", "-C", repo, "rev-parse", name], text=True).strip()
    subprocess.run(["git", "-C", repo, "update-ref", f"refs/remotes/origin/{name}", sha], check=True)
    subprocess.run(["git", "-C", repo, "branch", "-q", "-D", name], check=True)
for name in ["fix/empty-state", "feat/search", "fix/sync-folder"]:
    subprocess.run(["git", "-C", repo, "branch", "-q", "-d", name], check=True)
subprocess.run(["git", "-C", repo, "commit-graph", "write", "--reachable"], check=True)
