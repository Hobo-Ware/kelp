use gix::ObjectId;
use gix::hashtable::HashMap;
use gix::reference::Category;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Local,
    Remote,
    Tag,
}

#[derive(Debug, Clone)]
pub struct RefLabel {
    pub name: String,
    pub kind: RefKind,
    pub target: ObjectId,
    pub row: Option<u32>,
    pub is_head: bool,
    pub has_remote: bool,
}

#[derive(Debug, Default)]
pub struct Refs {
    pub labels: Vec<RefLabel>,
    pub head: Option<ObjectId>,
    pub head_branch: Option<String>,
    by_row: std::collections::HashMap<u32, Vec<usize>>,
}

impl Refs {
    pub fn load(repo: &gix::Repository) -> anyhow::Result<Self> {
        let head_branch = repo.head_name()?.map(|n| n.shorten().to_string());
        let head = repo.head_id().ok().map(|id| id.detach());
        let mut labels = Vec::new();
        for reference in repo.references()?.all()? {
            let Ok(mut reference) = reference else {
                continue;
            };
            let Some((category, short)) = reference.name().category_and_short_name() else {
                continue;
            };
            let kind = match category {
                Category::LocalBranch => RefKind::Local,
                Category::RemoteBranch if !short.ends_with(b"/HEAD") => RefKind::Remote,
                Category::Tag => RefKind::Tag,
                _ => continue,
            };
            let name = short.to_string();
            let Ok(commit) = reference.peel_to_commit() else {
                continue;
            };
            labels.push(RefLabel {
                is_head: kind == RefKind::Local && head_branch.as_deref() == Some(name.as_str()),
                name,
                kind,
                target: commit.id,
                row: None,
                has_remote: false,
            });
        }
        mark_tracked_remotes(&mut labels);
        labels.sort_by_key(|l| (!l.is_head, l.kind as u8, l.name.clone()));
        Ok(Self {
            labels,
            head,
            head_branch,
            by_row: Default::default(),
        })
    }

    pub fn tips(&self) -> Vec<ObjectId> {
        let mut tips: Vec<ObjectId> = self
            .labels
            .iter()
            .map(|l| l.target)
            .chain(self.head)
            .collect();
        tips.sort_unstable();
        tips.dedup();
        tips
    }

    pub fn attach_rows(&mut self, rows_by_id: &HashMap<ObjectId, u32>) {
        self.by_row.clear();
        for (i, label) in self.labels.iter_mut().enumerate() {
            label.row = rows_by_id.get(&label.target).copied();
            if let Some(row) = label.row
                && !(label.kind == RefKind::Remote && label.has_remote)
            {
                self.by_row.entry(row).or_default().push(i);
            }
        }
    }

    pub fn at_row(&self, row: usize) -> impl Iterator<Item = &RefLabel> {
        self.by_row
            .get(&(row as u32))
            .into_iter()
            .flatten()
            .map(|&i| &self.labels[i])
    }

    pub fn of_kind(&self, kind: RefKind) -> impl Iterator<Item = &RefLabel> {
        self.labels.iter().filter(move |l| l.kind == kind)
    }
}

fn mark_tracked_remotes(labels: &mut [RefLabel]) {
    let locals: std::collections::HashMap<String, ObjectId> = labels
        .iter()
        .filter(|l| l.kind == RefKind::Local)
        .map(|l| (l.name.clone(), l.target))
        .collect();
    let mut in_sync = Vec::new();
    for (i, label) in labels.iter().enumerate() {
        if label.kind != RefKind::Remote {
            continue;
        }
        let branch = label
            .name
            .split_once('/')
            .map_or(label.name.as_str(), |(_, b)| b);
        if locals.get(branch) == Some(&label.target) {
            in_sync.push((i, branch.to_string()));
        }
    }
    for (i, branch) in in_sync {
        labels[i].has_remote = true;
        if let Some(local) = labels
            .iter_mut()
            .find(|l| l.kind == RefKind::Local && l.name == branch)
        {
            local.has_remote = true;
        }
    }
}
