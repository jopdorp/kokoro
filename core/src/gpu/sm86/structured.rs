// Emits a structured plan inside one outer construct whose merge every exit reaches. Each Block and Loop is a SPIR-V
// loop construct, a Block's continue target unreachable, because SPIR-V 2.11 lets a branch leave a construct only for
// the innermost loop's merge or continue target: Br(0) is that branch, and a deeper Br stores the frame it names in a
// Function word and breaks, each construct it leaves checking the word at its merge and breaking or continuing on.
use super::cfg::{Ast, Pred};
use super::*;

struct Frame {
    merge: u32,
    cont: Option<u32>,
    left: bool,
}

impl Decoder<'_> {
    pub(super) fn translate_structured(&mut self, words: &[u128], plan: &[Ast]) {
        let len = self.ir.function_len();
        let target = self.ir.emit_variable(self.type_ptr_local, storage_class::FUNCTION);
        self.ir.move_tail(len, self.entry);
        self.locals = Some(Locals { vars: BTreeMap::new(), entry: self.entry, start: self.ir.function_len() });
        self.active = self.ir.emit_constant_true(self.type_bool[1]);
        self.ir.emit_store(target, self.const_u32_0);
        self.ir.emit_store(self.var_pc, self.const_u32_0);
        let [header, body, cont, merge] = [(); 4].map(|_| self.ir.alloc_id());
        let live = self.ir.emit_load(self.type_u32[1], self.var_live);
        let live = self.ir.emit_inot_equal(self.type_bool[1], live, self.const_u32_0);
        self.ir.emit_branch(header);
        self.ir.emit_label_id(header);
        self.ir.emit_loop_merge(merge, cont, 0);
        self.ir.emit_branch_conditional(live, body, merge);
        self.ir.emit_label_id(body);
        if !self.emit_sequence(words, plan, target, &mut vec![Frame { merge, cont: None, left: false }]) {
            self.ir.emit_branch(merge);
        }
        self.ir.emit_label_id(cont);
        self.ir.emit_branch(header);
        self.ir.emit_label_id(merge);
        for (slot, var) in self.locals.take().unwrap().vars {
            let value = self.ir.emit_load(self.type_u32[1], var);
            let ptr = self.aggregate_slot(slot);
            self.ir.emit_store(ptr, value);
        }
    }

    // Returns whether the sequence ended in a branch out of the block it leaves the emitter in.
    fn emit_sequence(&mut self, words: &[u128], seq: &[Ast], target: u32, frames: &mut Vec<Frame>) -> bool {
        for ast in seq {
            match ast {
                Ast::Code(range) => {
                    for pc in range.clone() {
                        self.pc = pc;
                        self.translate_guarded(words[pc as usize]);
                    }
                }
                Ast::If(preds, yes, no) => {
                    let cond = self.holds(preds);
                    let [then, otherwise, after] = [(); 3].map(|_| self.ir.alloc_id());
                    self.ir.emit_selection_merge(after, 0);
                    self.ir.emit_branch_conditional(cond, then, otherwise);
                    let mut ended = true;
                    for (branch, label) in [(yes, then), (no, otherwise)] {
                        self.ir.emit_label_id(label);
                        if !self.emit_sequence(words, branch, target, frames) {
                            self.ir.emit_branch(after);
                            ended = false;
                        }
                    }
                    self.ir.emit_label_id(after);
                    // SPIR-V 2.11: a merge block no path reaches only holds OpUnreachable.
                    if ended {
                        self.ir.emit_unreachable();
                        return true;
                    }
                }
                Ast::Block(body) | Ast::Loop(body) => {
                    let [header, start, cont, merge] = [(); 4].map(|_| self.ir.alloc_id());
                    self.ir.emit_branch(header);
                    self.ir.emit_label_id(header);
                    self.ir.emit_loop_merge(merge, cont, 0);
                    self.ir.emit_branch(start);
                    self.ir.emit_label_id(start);
                    frames.push(Frame { merge, cont: matches!(ast, Ast::Loop(_)).then_some(cont), left: false });
                    if !self.emit_sequence(words, body, target, frames) {
                        self.ir.emit_branch(merge);
                    }
                    let left = frames.pop().unwrap().left;
                    self.ir.emit_label_id(cont);
                    self.ir.emit_branch(header);
                    self.ir.emit_label_id(merge);
                    if left {
                        self.forward_pending(target, frames);
                    }
                }
                &Ast::Br(depth) => {
                    self.br(depth, target, frames);
                    return true;
                }
                Ast::Exit => {
                    self.exit(0);
                    self.br(frames.len() - 1, target, frames);
                    return true;
                }
            }
        }
        false
    }

    fn br(&mut self, depth: usize, target: u32, frames: &mut [Frame]) {
        let frame = frames.len() - 1 - depth;
        if depth > 0 {
            let id = self.cached_const_u32(frame as u32 + 1);
            self.ir.emit_store(target, id);
            frames[frame + 1..].iter_mut().for_each(|f| f.left = true);
        }
        let innermost = &frames[frames.len() - 1];
        self.ir.emit_branch(if depth == 0 { innermost.cont.unwrap_or(innermost.merge) } else { innermost.merge });
    }

    fn forward_pending(&mut self, target: u32, frames: &[Frame]) {
        let (bool_ty, ty) = (self.type_bool[1], self.type_u32[1]);
        let frame = frames.last().unwrap();
        let pending = self.ir.emit_load(ty, target);
        let here = self.cached_const_u32(frames.len() as u32);
        let here = self.ir.emit_iequal(bool_ty, pending, here);
        let kept = self.ir.emit_select(ty, here, self.const_u32_0, pending);
        let pending = self.ir.emit_inot_equal(bool_ty, pending, self.const_u32_0);
        let [handle, after] = [(); 2].map(|_| self.ir.alloc_id());
        self.ir.emit_selection_merge(after, 0);
        self.ir.emit_branch_conditional(pending, handle, after);
        self.ir.emit_label_id(handle);
        self.ir.emit_store(target, kept);
        match frame.cont {
            Some(cont) => self.ir.emit_branch_conditional(here, cont, frame.merge),
            None => self.ir.emit_branch(frame.merge),
        }
        self.ir.emit_label_id(after);
    }

    pub(super) fn holds(&mut self, preds: &[Pred]) -> u32 {
        let bits: Vec<u32> = preds.iter().map(|pred| {
            let bit = self.pred_bit(pred.uniform, pred.index, pred.not);
            self.ir.emit_inot_equal(self.type_bool[1], bit, self.const_u32_0)
        }).collect();
        bits.into_iter().reduce(|a, b| self.ir.emit_logical_and(self.type_bool[1], a, b)).unwrap()
    }
}
