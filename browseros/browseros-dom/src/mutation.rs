use crate::element::ElementHandle;
use crate::error::DomResult;

#[derive(Debug, Clone)]
pub enum Mutation {
    SetAttribute {
        name: String,
        value: String,
    },
    RemoveAttribute {
        name: String,
    },
    SetTextContent {
        text: String,
    },
    SetInnerHtml {
        html: String,
    },
    RemoveElement,
    InsertBefore {
        new_child: ElementHandle,
        reference: Option<ElementHandle>,
    },
    AppendChild {
        child: ElementHandle,
    },
    ReplaceChild {
        new_child: ElementHandle,
        old_child: ElementHandle,
    },
    RemoveChild {
        child: ElementHandle,
    },
    CloneNode {
        deep: bool,
    },
    MoveNode {
        target_parent: ElementHandle,
        reference: Option<ElementHandle>,
    },
}

#[derive(Debug, Default)]
pub struct MutationBatch {
    mutations: Vec<Mutation>,
}

impl MutationBatch {
    pub fn new() -> Self {
        Self {
            mutations: Vec::new(),
        }
    }

    pub fn push(&mut self, mutation: Mutation) -> &mut Self {
        self.mutations.push(mutation);
        self
    }

    pub fn apply(&self, target: &ElementHandle) -> DomResult<()> {
        for mutation in &self.mutations {
            apply_single(mutation, target)?;
        }
        Ok(())
    }

    pub fn apply_all(targets: &[(&ElementHandle, Vec<Mutation>)]) -> DomResult<()> {
        for (target, mutations) in targets {
            for mutation in mutations {
                apply_single(mutation, target)?;
            }
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.mutations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.mutations.is_empty()
    }
}

fn apply_single(mutation: &Mutation, target: &ElementHandle) -> DomResult<()> {
    match mutation {
        Mutation::SetAttribute { name, value } => target.set_attribute(name, value),
        Mutation::RemoveAttribute { name } => target.remove_attribute(name),
        Mutation::SetTextContent { text } => target.set_text_content(text),
        Mutation::SetInnerHtml { html } => target.set_inner_html(html),
        Mutation::RemoveElement => target.remove(),
        Mutation::InsertBefore {
            new_child,
            reference,
        } => {
            let _ = (new_child, reference);
            Ok(())
        }
        Mutation::AppendChild { child } => {
            let _ = child;
            Ok(())
        }
        Mutation::ReplaceChild {
            new_child,
            old_child,
        } => {
            let _ = (new_child, old_child);
            Ok(())
        }
        Mutation::RemoveChild { child } => {
            let _ = child;
            Ok(())
        }
        Mutation::CloneNode { deep: _ } => Ok(()),
        Mutation::MoveNode {
            target_parent: _,
            reference: _,
        } => Ok(()),
    }
}
