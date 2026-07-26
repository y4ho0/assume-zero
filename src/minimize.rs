use anyhow::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Minimized<T> {
    pub items: Vec<T>,
    pub complete: bool,
    pub evaluations: usize,
}

/// Finds a 1-minimal passing subset. `None` means the execution budget was exhausted.
pub fn ddmin<T: Clone + Eq>(
    candidates: &[T],
    mut predicate: impl FnMut(&[T]) -> Result<Option<bool>>,
) -> Result<Minimized<T>> {
    let mut evaluations = 0;
    evaluations += 1;
    if !matches!(predicate(candidates)?, Some(true)) {
        return Ok(Minimized {
            items: candidates.to_vec(),
            complete: false,
            evaluations,
        });
    }

    let mut current = candidates.to_vec();
    let mut partitions = 2_usize;
    while current.len() >= 2 {
        let chunk_size = current.len().div_ceil(partitions);
        let chunks: Vec<Vec<T>> = current.chunks(chunk_size).map(<[T]>::to_vec).collect();
        let mut reduced = false;

        for chunk_index in 0..chunks.len() {
            let complement: Vec<T> = chunks
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != chunk_index)
                .flat_map(|(_, values)| values.clone())
                .collect();
            evaluations += 1;
            match predicate(&complement)? {
                Some(true) => {
                    current = complement;
                    partitions = partitions.saturating_sub(1).max(2);
                    reduced = true;
                    break;
                }
                Some(false) => {}
                None => {
                    return Ok(Minimized {
                        items: current,
                        complete: false,
                        evaluations,
                    });
                }
            }
        }
        if reduced {
            continue;
        }

        for chunk in &chunks {
            evaluations += 1;
            match predicate(chunk)? {
                Some(true) => {
                    current = chunk.clone();
                    partitions = 2;
                    reduced = true;
                    break;
                }
                Some(false) => {}
                None => {
                    return Ok(Minimized {
                        items: current,
                        complete: false,
                        evaluations,
                    });
                }
            }
        }
        if reduced {
            continue;
        }
        if partitions >= current.len() {
            break;
        }
        partitions = (partitions * 2).min(current.len());
    }

    // Explicitly verify the promised 1-minimal property.
    let mut index = 0;
    while index < current.len() {
        let mut without = current.clone();
        without.remove(index);
        evaluations += 1;
        match predicate(&without)? {
            Some(true) => current = without,
            Some(false) => index += 1,
            None => {
                return Ok(Minimized {
                    items: current,
                    complete: false,
                    evaluations,
                });
            }
        }
    }

    Ok(Minimized {
        items: current,
        complete: true,
        evaluations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_one_minimal_pair() {
        let candidates = vec!["noise-a", "required-a", "noise-b", "required-b"];
        let result = ddmin(&candidates, |items| {
            Ok(Some(
                items.contains(&"required-a") && items.contains(&"required-b"),
            ))
        })
        .expect("ddmin");
        assert_eq!(result.items, vec!["required-a", "required-b"]);
        assert!(result.complete);
    }

    #[test]
    fn budget_termination_returns_current_best() {
        let candidates = vec![1, 2, 3, 4];
        let mut remaining = 2;
        let result = ddmin(&candidates, |_items| {
            if remaining == 0 {
                Ok(None)
            } else {
                remaining -= 1;
                Ok(Some(true))
            }
        })
        .expect("ddmin");
        assert!(!result.complete);
        assert!(!result.items.is_empty());
    }

    #[test]
    fn result_is_one_minimal() {
        let candidates = vec![1, 2, 3];
        let result = ddmin(&candidates, |items| Ok(Some(items.contains(&2)))).expect("ddmin");
        assert_eq!(result.items, vec![2]);
        for index in 0..result.items.len() {
            let mut without = result.items.clone();
            without.remove(index);
            assert!(!without.contains(&2));
        }
    }
}
