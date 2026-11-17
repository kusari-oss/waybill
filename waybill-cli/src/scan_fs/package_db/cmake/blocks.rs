//! Spec 1538 (#1519): the blocks enclosing a CMake command, read in file
//! order. Only what a fetch declaration needs: the conditions of every
//! enclosing `if`, and whether it sits in a `function()`/`macro()` body.
//! Nothing is evaluated.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Block {
    /// `if(<condition>)`. `elseif` replaces the condition; `else` prefixes
    /// the last one with `else: `.
    If { condition: String },
    Function { name: String },
    Loop,
}

#[derive(Debug, Default)]
pub(crate) struct BlockStack {
    stack: Vec<Block>,
    /// `end*` commands with no matching opener. Ignored, but counted.
    pub(crate) unbalanced: usize,
}

impl BlockStack {
    /// Apply one command. Returns `true` when it opened, changed or closed
    /// a block.
    pub(crate) fn observe(&mut self, name: &str, args: &[String]) -> bool {
        let text = args.join(" ");
        match name {
            "if" => self.stack.push(Block::If { condition: text }),
            "elseif" | "else" => match self.stack.last_mut() {
                Some(Block::If { condition }) => {
                    if name == "elseif" {
                        *condition = text;
                    } else if !condition.starts_with("else: ") {
                        *condition = format!("else: {condition}");
                    }
                }
                _ => self.unbalanced += 1,
            },
            "endif" => self.pop(|b| matches!(b, Block::If { .. })),
            "function" | "macro" => {
                self.stack.push(Block::Function { name: args.first().cloned().unwrap_or_default() })
            }
            "endfunction" | "endmacro" => self.pop(|b| matches!(b, Block::Function { .. })),
            "foreach" | "while" => self.stack.push(Block::Loop),
            "endforeach" | "endwhile" => self.pop(|b| matches!(b, Block::Loop)),
            _ => return false,
        }
        true
    }

    fn pop(&mut self, matches: impl Fn(&Block) -> bool) {
        if self.stack.last().is_some_and(matches) {
            self.stack.pop();
        } else {
            self.unbalanced += 1;
        }
    }

    /// The enclosing `if` conditions, outermost first.
    pub(crate) fn if_conditions(&self) -> Vec<String> {
        self.stack
            .iter()
            .filter_map(|b| match b {
                Block::If { condition } => Some(condition.clone()),
                _ => None,
            })
            .collect()
    }

    /// The innermost enclosing `function()`/`macro()`, if any.
    pub(crate) fn function(&self) -> Option<&str> {
        self.stack.iter().rev().find_map(|b| match b {
            Block::Function { name } => Some(name.as_str()),
            _ => None,
        })
    }

    /// What C232 records: the `if` conditions and `function: <name>` for
    /// a function or macro body, outermost first.
    pub(crate) fn conditions(&self) -> Vec<String> {
        self.stack
            .iter()
            .filter_map(|b| match b {
                Block::If { condition } => Some(condition.clone()),
                Block::Function { name } => Some(format!("function: {name}")),
                Block::Loop => None,
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn open(&self) -> usize {
        self.stack.len()
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn run(cmds: &[(&str, &str)]) -> BlockStack {
        let mut s = BlockStack::default();
        for (n, a) in cmds {
            let args: Vec<String> = a.split_whitespace().map(str::to_string).collect();
            s.observe(n, &args);
        }
        s
    }

    #[test]
    fn nested_if_conditions_are_outermost_first() {
        let s = run(&[("if", "NOT A"), ("if", "B AND C")]);
        assert_eq!(s.if_conditions(), vec!["NOT A".to_string(), "B AND C".to_string()]);
    }

    #[test]
    fn elseif_replaces_and_else_prefixes() {
        let s = run(&[("if", "A"), ("elseif", "B")]);
        assert_eq!(s.if_conditions(), vec!["B".to_string()]);
        let s = run(&[("if", "A"), ("else", "")]);
        assert_eq!(s.if_conditions(), vec!["else: A".to_string()]);
        let s = run(&[("if", "A"), ("else", ""), ("endif", "")]);
        assert_eq!(s.open(), 0);
    }

    #[test]
    fn a_function_body_inside_an_if_records_both() {
        let s = run(&[("if", "WIN32"), ("function", "wf_fetch name"), ("foreach", "x a b")]);
        assert_eq!(s.function(), Some("wf_fetch"));
        assert_eq!(s.conditions(), vec!["WIN32".to_string(), "function: wf_fetch".to_string()]);
        let s = run(&[("function", "wf_fetch"), ("endfunction", "")]);
        assert_eq!(s.function(), None);
    }

    #[test]
    fn unbalanced_ends_are_counted_not_fatal() {
        let s = run(&[("endif", ""), ("if", "A"), ("endforeach", "")]);
        assert_eq!(s.unbalanced, 2);
        assert_eq!(s.if_conditions(), vec!["A".to_string()]);
    }
}
