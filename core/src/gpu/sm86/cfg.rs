// The structured plan: the program's basic blocks and their terminators, dominators and natural loops, and from those
// Ramsey's recursive translation (N. Ramsey, "Beyond Relooper: recursive translation of unstructured control flow to
// structured control flow", ICFP 2022). A node that two forward edges reach, or that a loop exits to, follows a block
// its predecessors break out of, so lanes rejoin where the guest's paths do; a loop header heads a loop its back edges
// continue; every other node is inlined at its one branch. A program with an irreducible loop, or with a collective
// some lanes of its loop iteration can bypass, stays on the dispatcher, with the reason.
use super::*;
use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Translation {
    ForceDispatcher,
    #[default]
    Auto,
    ForceStructured,
}

#[derive(Clone, Copy, Debug)]
pub struct Pred {
    pub uniform: bool,
    pub index: u32,
    pub not: bool,
}

// Br(n) leaves the n-th enclosing Block or Loop counting from the innermost: it lands after a Block and continues a Loop.
#[derive(Debug)]
pub enum Ast {
    Code(Range<u32>),
    If(Vec<Pred>, Vec<Ast>, Vec<Ast>),
    Block(Vec<Ast>),
    Loop(Vec<Ast>),
    Br(usize),
    Exit,
}

enum Term {
    Goto(usize),
    Branch(Vec<Pred>, usize, usize),
}

struct Tree {
    idom: Vec<Option<usize>>,
    number: Vec<usize>,
}

impl Tree {
    fn new(succs: &[Vec<usize>], entry: usize) -> Self {
        let mut order = Vec::new();
        let mut number = vec![usize::MAX; succs.len()];
        let mut stack = vec![(entry, 0)];
        number[entry] = 0;
        while let Some((v, i)) = stack.pop() {
            match succs[v].get(i) {
                Some(&w) => {
                    stack.push((v, i + 1));
                    if number[w] == usize::MAX {
                        number[w] = 0;
                        stack.push((w, 0));
                    }
                }
                None => {
                    number[v] = order.len();
                    order.push(v);
                }
            }
        }
        let mut preds = vec![Vec::new(); succs.len()];
        for &v in &order {
            for &w in &succs[v] {
                preds[w].push(v);
            }
        }
        let mut tree = Self { idom: vec![None; succs.len()], number };
        tree.idom[entry] = Some(entry);
        let mut changed = true;
        while changed {
            changed = false;
            for &v in order.iter().rev().skip(1) {
                let idom = preds[v].iter().filter(|&&p| tree.idom[p].is_some()).fold(None, |acc, &p| Some(acc.map_or(p, |q| tree.common(p, q))));
                changed |= idom != tree.idom[v];
                tree.idom[v] = idom;
            }
        }
        tree
    }

    fn common(&self, mut a: usize, mut b: usize) -> usize {
        while a != b {
            while self.number[a] < self.number[b] {
                a = self.idom[a].unwrap();
            }
            while self.number[b] < self.number[a] {
                b = self.idom[b].unwrap();
            }
        }
        a
    }

    fn dominates(&self, a: usize, b: usize) -> bool {
        self.idom[b].is_some() && self.common(a, b) == a
    }
}

struct Builder {
    code: Vec<Range<u32>>,
    terms: Vec<Term>,
    dom: Tree,
    children: Vec<Vec<usize>>,
    follows: Vec<bool>,
    bodies: Vec<Vec<bool>>,
}

fn condition(inst: u128) -> Option<Pred> {
    if opcode(inst) == BRA_UPRED {
        return Some(Pred { uniform: true, index: (inst >> 24 & 7) as u32, not: inst >> 27 & 1 != 0 });
    }
    (matches!(opcode(inst), BRA | BRA_UREG) && inst >> 87 & 0xf != PT as u128).then(|| Pred { uniform: is_uniform(inst), index: (inst >> 87 & 7) as u32, not: inst >> 90 & 1 != 0 })
}

fn ends_block(inst: u128) -> bool {
    let branch = matches!(opcode(inst), BRA | BRA_UPRED | BRA_UREG) && inst >> 32 & 3 != 2 || matches!(opcode(inst), CALL_ABS | CALL_REL | JMP_ABS);
    inst >> 12 & 0xf != 0xf && (branch || exits(inst))
}

fn exits(inst: u128) -> bool {
    matches!(opcode(inst), EXIT | KILL | 0x95a | 0xb5a | 0x35a | 0x1b5a | 0x1d5a)
}

fn unsupported(pc: usize, inst: u128, collectives: bool) -> Option<String> {
    let op = opcode(inst);
    let reason = if !collectives && (exchanges(inst) || partial_warpsync(inst)) || matches!(op, 0x198c | 0x1f8c | 0x38c | 0x38d) {
        "collective"
    } else if dynamic_target(inst) || matches!(op, RET | 0x1950) {
        "dynamic branch target"
    } else if matches!(op, 0x1478 | 0x1878 | 0x1a78 | 0x1c78) {
        "indexed register"
    } else if op == KILL {
        "KILL"
    } else {
        return None;
    };
    Some(format!("{reason} {op:#x} at pc {pc}"))
}

pub fn plan(words: &[u128]) -> Result<Vec<Ast>, String> {
    plan_with_collectives(words, false, false)
}

// A fragment's KILL demotes it to a helper invocation that runs on, so it ends no block there.
pub(super) fn plan_with_collectives(words: &[u128], collectives: bool, demotes: bool) -> Result<Vec<Ast>, String> {
    let ends_block = |inst: u128| ends_block(inst) && !(demotes && opcode(inst) == KILL);
    if let Some(reason) = (0..).zip(words).find_map(|(pc, &inst)| unsupported(pc, inst, collectives)) {
        return Err(reason);
    }
    let len = words.len() as u32;
    let mut starts = vec![0, len];
    for (pc, &inst) in (0..).zip(words).filter(|&(_, &inst)| ends_block(inst)) {
        starts.push(pc + 1);
        starts.extend(branch_target(pc, inst));
    }
    starts.sort_unstable();
    starts.dedup();
    if starts.last() != Some(&len) {
        return Err("branch target outside the program".into());
    }
    let exit = starts.len() - 1;
    let node = |pc: u32| starts.binary_search(&pc).unwrap();
    let mut terms = Vec::new();
    let mut code = Vec::new();
    for window in starts.windows(2) {
        let (start, end) = (window[0], window[1]);
        let last = words[end as usize - 1];
        let fall = node(end);
        let (term, code_end) = if !ends_block(last) {
            (Term::Goto(fall), end)
        } else {
            let target = if exits(last) { exit } else { node(branch_target(end - 1, last).unwrap()) };
            let preds: Vec<Pred> = guard_pred(last).into_iter().chain(condition(last)).collect();
            (if preds.is_empty() { Term::Goto(target) } else { Term::Branch(preds, target, fall) }, end - 1)
        };
        terms.push(term);
        code.push(start..code_end);
    }
    let mut succs: Vec<Vec<usize>> = terms.iter().map(|term| match *term {
        Term::Goto(t) => vec![t],
        Term::Branch(_, t, f) => vec![t, f],
    }).collect();
    succs.push(Vec::new());
    let dom = Tree::new(&succs, 0);
    let reachable = |v: usize| dom.idom[v].is_some();
    let n = succs.len();
    let mut reverse = vec![Vec::new(); n];
    let mut latches = vec![Vec::new(); n];
    let mut forward = vec![0; n];
    for v in (0..exit).filter(|&v| reachable(v)) {
        for &w in &succs[v] {
            reverse[w].push(v);
            if dom.number[w] < dom.number[v] {
                forward[w] += 1;
            } else if dom.dominates(w, v) {
                latches[w].push(v);
            } else {
                return Err(format!("irreducible loop into pc {}", starts[w]));
            }
        }
    }
    // An outer header precedes its inner ones in reverse postorder, so the innermost loop of a node is assigned last.
    let mut headers: Vec<usize> = (0..exit).filter(|&h| !latches[h].is_empty()).collect();
    headers.sort_by_key(|&h| std::cmp::Reverse(dom.number[h]));
    let mut bodies = vec![Vec::new(); n];
    let mut inner = vec![None; n];
    let mut outer = vec![None; n];
    for &h in &headers {
        let mut body = vec![false; n];
        body[h] = true;
        let mut work = latches[h].clone();
        while let Some(v) = work.pop() {
            if !std::mem::replace(&mut body[v], true) {
                work.extend(&reverse[v]);
            }
        }
        outer[h] = inner[h];
        (0..n).filter(|&v| body[v]).for_each(|v| inner[v] = Some(h));
        bodies[h] = body;
    }
    let mut children = vec![Vec::new(); n];
    let mut follows: Vec<bool> = forward.iter().map(|&count| count > 1).collect();
    for t in (1..exit).filter(|&t| reachable(t)) {
        let mut parent = dom.idom[t].unwrap();
        let mut ring = inner[parent];
        while let Some(h) = ring {
            if !bodies[h][t] {
                parent = h;
                follows[t] = true;
            }
            ring = outer[h];
        }
        children[parent].push(t);
    }
    for (v, pc) in (0..exit).filter(|&v| reachable(v)).flat_map(|v| code[v].clone().map(move |pc| (v, pc))) {
        let inst = words[pc as usize];
        if !(exchanges(inst) || partial_warpsync(inst)) { continue; }
        // Each loop iteration runs in lockstep, so a lane may not skip a collective and start the next iteration while
        // the others are still on their way to it.
        let h = inner[v].unwrap_or(0);
        let mut seen = vec![false; n];
        let mut work = vec![h];
        while let Some(u) = work.pop().filter(|_| v != h) {
            for &w in succs[u].iter().filter(|&&w| w != v && w != exit && inner[v].is_none_or(|l| bodies[l][w])) {
                if w == h {
                    return Err(format!("collective {:#x} at pc {pc} that lanes of its iteration can bypass", opcode(inst)));
                }
                if !std::mem::replace(&mut seen[w], true) { work.push(w); }
            }
        }
    }
    let builder = Builder { code, terms, dom, children, follows, bodies };
    builder.tree(0, &mut Vec::new())
}

#[derive(Clone, Copy, PartialEq)]
enum Frame {
    Loop(usize),
    Follow(usize),
}

impl Builder {
    fn tree(&self, x: usize, frames: &mut Vec<Frame>) -> Result<Vec<Ast>, String> {
        let mut merges: Vec<usize> = self.children[x].iter().copied().filter(|&t| self.follows[t]).collect();
        merges.sort_by_key(|&t| std::cmp::Reverse(self.dom.number[t]));
        if self.bodies[x].is_empty() {
            return self.within(&merges, frames, |b, frames| b.node(x, frames));
        }
        let (inside, after): (Vec<usize>, Vec<usize>) = merges.iter().partition(|&&t| self.bodies[x][t]);
        self.within(&after, frames, |b, frames| {
            frames.push(Frame::Loop(x));
            let body = b.within(&inside, frames, |b, frames| b.node(x, frames));
            frames.pop();
            Ok(vec![Ast::Loop(body?)])
        })
    }

    // Each merge follows a block around the code before it, the latest in reverse postorder outermost.
    fn within(&self, merges: &[usize], frames: &mut Vec<Frame>, code: impl Fn(&Self, &mut Vec<Frame>) -> Result<Vec<Ast>, String>) -> Result<Vec<Ast>, String> {
        let Some((&last, rest)) = merges.split_last() else { return code(self, frames) };
        frames.push(Frame::Follow(last));
        let block = self.within(rest, frames, code);
        frames.pop();
        let mut seq = vec![Ast::Block(block?)];
        seq.extend(self.tree(last, frames)?);
        Ok(seq)
    }

    fn node(&self, x: usize, frames: &mut Vec<Frame>) -> Result<Vec<Ast>, String> {
        let mut seq: Vec<Ast> = Some(self.code[x].clone()).filter(|code| !code.is_empty()).map(Ast::Code).into_iter().collect();
        match &self.terms[x] {
            Term::Goto(t) => seq.extend(self.branch(x, *t, frames)?),
            Term::Branch(preds, t, f) => {
                let yes = self.branch(x, *t, frames)?;
                seq.push(Ast::If(preds.clone(), yes, self.branch(x, *f, frames)?));
            }
        }
        Ok(seq)
    }

    fn branch(&self, x: usize, t: usize, frames: &mut Vec<Frame>) -> Result<Vec<Ast>, String> {
        let depth = |frame| match frames.iter().rev().position(|&f| f == frame) {
            Some(depth) => Ok(vec![Ast::Br(depth)]),
            None => Err(format!("branch to pc {} outside every enclosing block", self.code[t].start)),
        };
        if t == self.terms.len() {
            Ok(vec![Ast::Exit])
        } else if self.dom.number[t] >= self.dom.number[x] {
            depth(Frame::Loop(t))
        } else if self.follows[t] {
            depth(Frame::Follow(t))
        } else {
            self.tree(t, frames)
        }
    }
}
