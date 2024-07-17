//! The proof object model for RPN proofs used in Metamath.

use crate::statement::StatementAddress;
use crate::util::HashMap;
use crate::verify::ProofBuilder;
use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::collections::BinaryHeap;
use std::hash::{Hash, Hasher};
use std::ops::Range;

/// A tree structure for storing proofs and grammar derivations.
#[derive(Clone, Debug, Eq)]
pub struct ProofTree {
    /// The axiom/theorem being applied at the root.
    pub address: StatementAddress,
    /// The hypotheses ($e and $f) in database order, indexes into the parent `ProofTreeArray`.
    pub children: Vec<usize>,
    /// The precomputed hash for this tree.
    hash: u64,
}

impl PartialEq for ProofTree {
    /// This is a shallow equality check
    fn eq(&self, other: &ProofTree) -> bool {
        self.address == other.address && self.children == other.children
    }
}

impl Hash for ProofTree {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.hash.hash(state)
    }
}

impl ProofTree {
    /// Create a new proof tree using the given atom and children.
    #[must_use]
    pub fn new(parent: &ProofTreeArray, address: StatementAddress, children: Vec<usize>) -> Self {
        let mut hasher = DefaultHasher::new();
        address.hash(&mut hasher);
        for &ix in &children {
            parent.trees[ix].hash(&mut hasher);
        }
        ProofTree {
            address,
            children,
            hash: hasher.finish(),
        }
    }
}

/// An array of proof trees, used to collect steps of a proof
/// in proof order
#[derive(Debug, Clone)]
pub struct ProofTreeArray {
    map: HashMap<u64, usize>,
    /// The list of proof trees
    pub trees: Vec<ProofTree>,
    /// The uncompressed strings for each proof tree.
    /// Set this to `None` to disable expression construction
    exprs: Option<Vec<Vec<u8>>>,
    /// The QED step
    pub qed: usize,
    /// The distance from each step to the QED step
    indent: Vec<u16>,
}

impl Default for ProofTreeArray {
    fn default() -> Self {
        Self::new(true)
    }
}

/// A strongly typed representation of the RPN proof style used by
/// Metamath proofs (except compressed style)
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RPNStep {
    /// A "normal" step is one that defines a new formula not seen
    /// before in this proof.
    Normal {
        /// A number by which to refer to this step in later steps,
        /// or zero if it is not going to be reused.
        fwdref: usize,
        /// The theorem being applied at this step
        addr: StatementAddress,
        /// The address of the parent of this step, and the index of
        /// this hypotheses among its siblings. This is set to `None`
        /// if we are not in explicit mode, and it is also `None` for
        /// the root step (which has no parent).
        hyp: Option<(StatementAddress, usize)>,
    },
    /// A backreference step, which copies the subtree and formula
    /// from a previously derived subtree.
    Backref {
        /// A reference to the previously numbered step
        backref: usize,
        /// The address of the parent of this step, and the index of
        /// this hypotheses among its siblings. This is set to `None`
        /// if we are not in explicit mode, and it is also `None` for
        /// the root step (which has no parent).
        hyp: Option<(StatementAddress, usize)>,
    },
}

impl ProofTreeArray {
    /// Constructs a new empty `ProofTreeArray`. If `enable_exprs` is true,
    /// it will construct expressions for each step, used by [`Database::export_mmp_proof_tree`].
    #[must_use]
    pub fn new(enable_exprs: bool) -> Self {
        Self {
            map: HashMap::default(),
            trees: vec![],
            exprs: if enable_exprs { Some(vec![]) } else { None },
            qed: 0,
            indent: vec![],
        }
    }

    /// Get the index of a proof tree in the array
    #[must_use]
    pub fn index(&self, tree: &ProofTree) -> Option<usize> {
        self.map.get(&tree.hash).copied()
    }


    /// Get the minimum distance from each step to the QED step
    #[must_use]
    pub fn indent(&self) -> &[u16] {
        &self.indent
    }

    /// Finds the shortest path from each node in the proof tree to the `qed` step,
    /// using Dijkstra's algorithm.  Based on the example in
    /// <https://doc.rust-lang.org/std/collections/binary_heap/>.
    pub fn calc_indent(&mut self) {
        #[derive(Copy, Clone, Eq, PartialEq)]
        struct IndentNode {
            index: usize,
            cost: u16,
        }

        // The priority queue depends on `Ord`.
        // Explicitly implement the trait so the queue becomes a min-heap
        // instead of a max-heap.
        impl Ord for IndentNode {
            fn cmp(&self, other: &IndentNode) -> Ordering {
                // Notice that the we flip the ordering here
                other.cost.cmp(&self.cost)
            }
        }

        // `PartialOrd` needs to be implemented as well.
        impl PartialOrd for IndentNode {
            fn partial_cmp(&self, other: &IndentNode) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        // dist[node] = current shortest distance from `start` to `node`
        let mut dist: Vec<u16> = vec![u16::MAX; self.trees.len()];

        let mut heap = BinaryHeap::new();

        // We're at `qed`, with a zero cost
        dist[self.qed] = 0;
        heap.push(IndentNode {
            index: self.qed,
            cost: 0,
        });

        // Examine the frontier with lower cost nodes first (min-heap)
        while let Some(IndentNode { index, cost }) = heap.pop() {
            // Important as we may have already found a better way
            if cost > dist[index] {
                continue;
            }

            // For each node we can reach, see if we can find a way with
            // a lower cost going through this node
            for &hix in &self.trees[index].children {
                let next = IndentNode {
                    index: hix,
                    cost: cost + 1,
                };

                // If so, add it to the frontier and continue
                if next.cost < dist[next.index] {
                    heap.push(next);
                    // Relaxation, we have now found a better way
                    dist[next.index] = next.cost;
                }
            }
        }

        self.indent = dist;
    }

    /// Get the number of parents of each step in the proof
    #[must_use]
    pub fn count_parents(&self) -> Vec<usize> {
        let mut out = vec![0; self.trees.len()];
        for tree in &self.trees {
            for &hix in &tree.children {
                out[hix] += 1;
            }
        }
        out
    }

    /// Write the proof as an RPN sequence with backrefs
    #[must_use]
    pub fn to_rpn(&self, parents: &[usize], explicit: bool) -> Vec<RPNStep> {
        #[derive(Debug)]
        struct Env<'a> {
            arr: &'a ProofTreeArray,
            parents: &'a [usize],
            explicit: bool,
            out: Vec<RPNStep>,
            backrefs: Vec<usize>,
            count: usize,
        }

        fn output_step(env: &mut Env<'_>, step: usize, hyp: Option<(StatementAddress, usize)>) {
            let step = if env.backrefs[step] == 0 {
                let tree = &env.arr.trees[step];
                for (i, &hix) in tree.children.iter().enumerate() {
                    let n_hyp = if env.explicit {
                        Some((tree.address, i))
                    } else {
                        None
                    };
                    output_step(env, hix, n_hyp);
                }
                RPNStep::Normal {
                    fwdref: if env.parents[step] > 1 && !tree.children.is_empty() {
                        env.count += 1;
                        env.backrefs[step] = env.count;
                        env.count
                    } else {
                        0
                    },
                    addr: tree.address,
                    hyp,
                }
            } else {
                RPNStep::Backref {
                    backref: env.backrefs[step],
                    hyp,
                }
            };
            env.out.push(step);
        }
        let mut env = Env {
            arr: self,
            parents,
            explicit,
            out: vec![],
            backrefs: vec![0; self.trees.len()],
            count: 0,
        };
        output_step(&mut env, self.qed, None);
        env.out
    }

    /// Produce an iterator over the steps in the proof in
    /// normal/uncompressed mode. (Because this can potentially
    /// be *very* long, we do not store the list and just stream it.)
    #[must_use]
    pub fn normal_iter(&self, explicit: bool) -> NormalIter<'_> {
        NormalIter {
            arr: self,
            explicit,
            stack: vec![(self.qed, 0)],
        }
    }

    /// Returns the list of expressions corresponding to each proof tree.
    #[must_use]
    pub fn exprs(&self) -> Option<&[Vec<u8>]> {
        self.exprs.as_deref()
    }
}

/// An iterator which loops over the steps of the proof in tree order
/// (with repetition for duplicate subtrees).
#[derive(Debug)]
pub struct NormalIter<'a> {
    arr: &'a ProofTreeArray,
    explicit: bool,
    stack: Vec<(usize, usize)>,
}

impl<'a> Iterator for NormalIter<'a> {
    type Item = RPNStep;

    fn next(&mut self) -> Option<RPNStep> {
        loop {
            let (ix, ohix) = match self.stack.last() {
                None => return None,
                Some(&(ix, child)) => (ix, self.arr.trees[ix].children.get(child)),
            };
            if let Some(&hix) = ohix {
                self.stack.push((hix, 0));
                continue;
            }
            self.stack.pop();
            let hyp = if let Some(&mut (lix, ref mut i)) = self.stack.last_mut() {
                let hyp = if self.explicit {
                    Some((self.arr.trees[lix].address, *i))
                } else {
                    None
                };
                *i += 1;
                hyp
            } else {
                None
            };
            let out = RPNStep::Normal {
                fwdref: 0,
                addr: self.arr.trees[ix].address,
                hyp,
            };
            return Some(out);
        }
    }
}

impl ProofBuilder for ProofTreeArray {
    type Item = usize;
    type Accum = Vec<usize>;

    fn push(&mut self, hyps: &mut Vec<usize>, hyp: usize) {
        hyps.push(hyp);
    }

    fn build(
        &mut self,
        addr: StatementAddress,
        trees: Vec<usize>,
        pool: &[u8],
        expr: Range<usize>,
    ) -> usize {
        let tree = ProofTree::new(self, addr, trees);
        self.index(&tree).unwrap_or_else(|| {
            let ix = self.trees.len();
            self.map.insert(tree.hash, ix);
            self.trees.push(tree);
            if let Some(exprs) = &mut self.exprs {
                let mut u_expr = vec![b' '];
                for &chr in &pool[expr] {
                    if chr & 0x80 == 0 {
                        u_expr.push(chr);
                    } else {
                        u_expr.push(chr & 0x7F);
                        u_expr.push(b' ');
                    }
                }
                u_expr.pop();
                exprs.push(u_expr);
            }
            ix
        })
    }
}

/// List of possible proof output types.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ProofStyle {
    /// `/compressed` proof output (default). Label list followed by step letters.
    Compressed,
    /// `/normal` proof output. Uncompressed step names only.
    Normal,
    /// `/packed` proof output. Same as `/normal` with backreferences.
    Packed,
    /// `/explicit` proof output. `/normal` with hypothesis names.
    Explicit,
    /// `/packed/explicit` proof output. `/normal` with hypothesis names and backreferences.
    PackedExplicit,
}

impl ProofStyle {
    /// Returns `true` if this is in explicit style (showing proof hypotheses labels
    /// on each step)
    #[must_use]
    pub const fn explicit(self) -> bool {
        matches!(self, ProofStyle::Explicit | ProofStyle::PackedExplicit)
    }

    /// Returns `true` if this is in packed style, meaning duplicate subtrees are
    /// referred to by backreferences instead of inlined. (Compressed proofs are
    /// considered packed by this definition.)
    #[must_use]
    pub const fn packed(self) -> bool {
        matches!(
            self,
            ProofStyle::Compressed | ProofStyle::Packed | ProofStyle::PackedExplicit
        )
    }
}
