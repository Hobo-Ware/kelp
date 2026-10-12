use kelp_core::ops::Op;
use kelp_core::refs::{RefKind, RefLabel};
use kelp_ui::graph::DropTarget;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DropChoice {
    Merge { source: String, into: String },
    CheckoutAndMerge { branch: String, source: String },
    Rebase { current: String, onto: String },
    Reset { current: String, commit: String },
}

pub fn target_name(target: &DropTarget) -> String {
    target
        .label
        .as_ref()
        .map_or_else(|| short(&target.commit).to_string(), |l| l.name.clone())
}

pub fn drop_choices(
    source: &RefLabel,
    target: &DropTarget,
    current: Option<&str>,
) -> Vec<DropChoice> {
    if target
        .label
        .as_ref()
        .is_some_and(|l| l.name == source.name && l.kind == source.kind)
    {
        return Vec::new();
    }
    let mut choices = Vec::new();
    let target_branch = target
        .label
        .as_ref()
        .filter(|l| l.kind == RefKind::Local)
        .map(|l| l.name.as_str());
    match (target_branch, current) {
        (Some(branch), Some(current)) if branch == current => choices.push(DropChoice::Merge {
            source: source.name.clone(),
            into: branch.to_string(),
        }),
        (Some(branch), _) => choices.push(DropChoice::CheckoutAndMerge {
            branch: branch.to_string(),
            source: source.name.clone(),
        }),
        (None, Some(current)) if target.is_head && source.name != current => {
            choices.push(DropChoice::Merge {
                source: source.name.clone(),
                into: current.to_string(),
            })
        }
        _ => {}
    }
    if let Some(current) = current
        && !target.is_head
    {
        let onto = target_name(target);
        if onto != current {
            choices.push(DropChoice::Rebase {
                current: current.to_string(),
                onto,
            });
        }
        choices.push(DropChoice::Reset {
            current: current.to_string(),
            commit: target.commit.clone(),
        });
    }
    choices
}

pub fn checkout_op(label: &RefLabel) -> Option<Op> {
    match label.kind {
        RefKind::Local if !label.is_head => Some(Op::Switch(label.name.clone())),
        RefKind::Remote => Some(Op::SwitchTrack(label.name.clone())),
        _ => None,
    }
}

fn short(commit: &str) -> &str {
    &commit[..7.min(commit.len())]
}

#[cfg(test)]
mod tests {
    use kelp_core::refs::{RefKind, RefLabel};

    use super::*;

    fn label(name: &str, kind: RefKind, is_head: bool) -> RefLabel {
        RefLabel {
            name: name.into(),
            kind,
            target: gix::ObjectId::null(gix::hash::Kind::Sha1),
            row: Some(0),
            is_head,
            has_remote: false,
            hidden: false,
            pull: None,
        }
    }

    fn target(label: Option<RefLabel>, is_head: bool) -> DropTarget {
        DropTarget {
            row: 3,
            commit: "abcdef1234567890".into(),
            label,
            is_head,
        }
    }

    #[test]
    fn dropping_on_the_checked_out_branch_merges_into_it() {
        let src = label("feat/a", RefKind::Local, false);
        let main = label("main", RefKind::Local, true);
        let choices = drop_choices(&src, &target(Some(main), true), Some("main"));
        assert_eq!(
            choices,
            vec![DropChoice::Merge {
                source: "feat/a".into(),
                into: "main".into()
            }]
        );
    }

    #[test]
    fn dropping_on_another_branch_checks_it_out_first() {
        let src = label("feat/a", RefKind::Local, false);
        let release = label("release", RefKind::Local, false);
        let choices = drop_choices(&src, &target(Some(release), false), Some("main"));
        assert_eq!(
            choices[0],
            DropChoice::CheckoutAndMerge {
                branch: "release".into(),
                source: "feat/a".into()
            }
        );
        assert_eq!(
            choices[1],
            DropChoice::Rebase {
                current: "main".into(),
                onto: "release".into()
            }
        );
        assert!(matches!(choices[2], DropChoice::Reset { .. }));
    }

    #[test]
    fn dropping_on_a_plain_commit_rebases_or_resets_by_hash() {
        let src = label("main", RefKind::Local, true);
        let choices = drop_choices(&src, &target(None, false), Some("main"));
        assert_eq!(
            choices,
            vec![
                DropChoice::Rebase {
                    current: "main".into(),
                    onto: "abcdef1".into()
                },
                DropChoice::Reset {
                    current: "main".into(),
                    commit: "abcdef1234567890".into()
                }
            ]
        );
    }

    #[test]
    fn dropping_on_head_merges_and_nothing_else() {
        let src = label("origin/feat/a", RefKind::Remote, false);
        let choices = drop_choices(&src, &target(None, true), Some("main"));
        assert_eq!(
            choices,
            vec![DropChoice::Merge {
                source: "origin/feat/a".into(),
                into: "main".into()
            }]
        );
    }

    #[test]
    fn dropping_a_label_on_itself_does_nothing() {
        let src = label("feat/a", RefKind::Local, false);
        let same = label("feat/a", RefKind::Local, false);
        assert_eq!(
            drop_choices(&src, &target(Some(same), false), Some("main")),
            Vec::<DropChoice>::new()
        );
    }

    #[test]
    fn detached_head_only_offers_checkout_and_merge() {
        let src = label("feat/a", RefKind::Local, false);
        let release = label("release", RefKind::Local, false);
        assert_eq!(
            drop_choices(&src, &target(Some(release), false), None),
            vec![DropChoice::CheckoutAndMerge {
                branch: "release".into(),
                source: "feat/a".into()
            }]
        );
    }

    #[test]
    fn the_head_label_has_nothing_to_check_out() {
        let head = label("main", RefKind::Local, true);
        assert_eq!(checkout_op(&head), None);
        let other = label("feat/a", RefKind::Local, false);
        assert_eq!(checkout_op(&other), Some(Op::Switch("feat/a".into())));
        let remote = label("origin/feat/a", RefKind::Remote, false);
        assert_eq!(
            checkout_op(&remote),
            Some(Op::SwitchTrack("origin/feat/a".into()))
        );
    }
}
