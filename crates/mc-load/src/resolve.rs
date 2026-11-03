//! Name resolution of one lowered statement (decisions F.1 to F.3): a rewrite of its symbols that keeps every
//! locally bound name, using `mc-syntax`'s [`scope`](mc_syntax::scope) helpers on the same binding forms as
//! lowering and the printer.
//!
//! The walk recurses once per term level. Lowered statements come from parsed text, whose nesting the parser caps
//! at [`mc_syntax::MAX_DEPTH`], and lowering adds at most a few term levels per syntax level, so the depth is
//! bounded by a small constant times that cap (the evaluator's terms, which can be deep, are never resolved).
use crate::modules::{qualify, split_qualified, Imports, Table};
use mc_api::{names, Store, Sym, View, T};
use mc_syntax::scope::{lambda_param_names, pattern_vars, statement_binding, Scopes};
use mc_syntax::{as_sym, Head, Heads};
use std::collections::{BTreeSet, HashMap, HashSet};

/// A resolution error: the message and the name it is about (to find its position in the statement).
#[derive(Debug)]
pub(crate) struct Fail {
    pub name: String,
    pub message: String,
}

impl Fail {
    pub(crate) fn new(name: impl Into<String>, message: impl Into<String>) -> Fail {
        Fail { name: name.into(), message: message.into() }
    }
}

/// Where a statement is resolved.
pub(crate) enum Ctx<'a> {
    /// The top level of a session input or file, with its imports.
    Top { imports: &'a Imports },
    /// The body of `module name { ... }`.
    Module {
        name: &'a str,
        /// the names defined at the top level of the body (forward references work)
        members: &'a BTreeSet<String>,
        /// the module's own imports
        imports: &'a Imports,
        /// bare names defined at the top level of the source around the module; they count as known to the
        /// kernel although they are not evaluated yet (decision F.11)
        defined: &'a HashSet<Sym>,
    },
}

/// What a resolution needs besides the context; shared by every statement of one read.
pub(crate) struct Env<'a, 'h, S: Store + ?Sized> {
    pub store: &'h S,
    pub heads: &'a Heads,
    pub modules: &'a Table,
    /// whether the kernel knows the symbol (a native, rule, attribute or own value)
    pub known: &'a dyn Fn(Sym) -> bool,
    /// the lowering heads and the contract's names: always known
    pub reserved: &'a HashSet<Sym>,
}

/// The symbols that are known whatever the kernel holds: the heads lowering produces and the names fixed by the
/// contract (`true`, `false`, `null`, `failure` and its tags).
pub(crate) fn reserved<S: Store + ?Sized>(store: &S) -> HashSet<Sym> {
    use names::failure::*;
    let contract = [
        names::LIST,
        names::ADD,
        names::MUL,
        names::POW,
        names::COMPLEX,
        names::FAILURE,
        names::RULE,
        names::RULE_DELAYED,
        names::TRUE,
        names::FALSE,
        names::NULL,
        DIVISION_BY_ZERO,
        LIMIT,
        MEMORY,
        FUEL,
        TIME,
        INTERRUPTED,
        NO_MATCH,
        EFFECT_IN_CONDITION,
        DEPTH,
        DEFINITION,
    ];
    Head::ALL.iter().map(|h| h.name()).chain(contract).map(|n| store.sym(n)).collect()
}

/// The resolver of one statement.
pub(crate) struct Walk<'e, 'a, 'h, S: Store + ?Sized> {
    env: &'e Env<'a, 'h, S>,
    ctx: Ctx<'e>,
    scopes: Scopes,
    /// resolutions of names that are not locally bound (they depend only on the context)
    cache: HashMap<Sym, Sym>,
    failure: Sym,
}

type R<'h> = Result<T<'h>, Fail>;

impl<'e, 'a, 'h, S: Store + ?Sized> Walk<'e, 'a, 'h, S> {
    pub(crate) fn new(env: &'e Env<'a, 'h, S>, ctx: Ctx<'e>) -> Self {
        let failure = env.store.sym(names::FAILURE);
        Walk { env, ctx, scopes: Scopes::default(), cache: HashMap::new(), failure }
    }

    /// The statement `t` with every name resolved.
    pub(crate) fn statement(&mut self, t: T<'h>) -> R<'h> {
        self.term(t)
    }

    fn term(&mut self, t: T<'h>) -> R<'h> {
        let st = self.env.store;
        match st.view(t) {
            View::Sym(s) => {
                let r = self.sym(s)?;
                Ok(if r == s { t } else { st.sym_term(r) })
            }
            View::App { head, args } => self.app(t, head, args),
            _ => Ok(t),
        }
    }

    /// The resolution of symbol `s` where it occurs (decision F.2).
    fn sym(&mut self, s: Sym) -> Result<Sym, Fail> {
        if self.scopes.lookup(s).is_some() {
            return Ok(s);
        }
        if let Some(&r) = self.cache.get(&s) {
            return Ok(r);
        }
        let r = self.global(s)?;
        self.cache.insert(s, r);
        Ok(r)
    }

    /// The resolution of a name that is not locally bound.
    fn global(&self, s: Sym) -> Result<Sym, Fail> {
        let env = self.env;
        let name = env.store.sym_name(s);
        if let Some((module, member)) = split_qualified(&name) {
            self.qualified(&name, module, member)?;
            return Ok(s);
        }
        let imported = |imports: &Imports| -> Result<Option<Sym>, Fail> {
            match imports.get(&name) {
                None => Ok(None),
                Some([m]) => Ok(Some(env.store.sym(&qualify(m, &name)))),
                Some(ms) => Err(Fail::new(
                    name.clone(),
                    format!("`{name}` is ambiguous: it is imported from modules {}", ms.join(" and ")),
                )),
            }
        };
        match self.ctx {
            Ctx::Top { .. } if env.reserved.contains(&s) => Ok(s),
            Ctx::Top { imports } => Ok(imported(imports)?.unwrap_or(s)),
            Ctx::Module { name: m, members, imports, defined } => {
                // a lowering head stands for syntax (`+` is `add`): never a member (decision F.15)
                if env.reserved.contains(&s) {
                    return Ok(s);
                }
                if members.contains(&name) {
                    return Ok(env.store.sym(&qualify(m, &name)));
                }
                if let Some(r) = imported(imports)? {
                    return Ok(r);
                }
                if defined.contains(&s) || (env.known)(s) {
                    return Ok(s);
                }
                // a fresh symbol of the module (04 section 6.4)
                Ok(env.store.sym(&qualify(m, &name)))
            }
        }
    }

    /// Checks a qualified name `module.member` (decision F.3).
    fn qualified(&self, name: &str, module: &str, member: &str) -> Result<(), Fail> {
        if let Ctx::Module { name: m, members, .. } = self.ctx {
            if m == module {
                return if members.contains(member) {
                    Ok(())
                } else {
                    Err(Fail::new(name, format!("module {module} has no member `{member}`")))
                };
            }
        }
        match self.env.modules.get(module) {
            Some(md) if md.public.contains(member) => Ok(()),
            Some(md) if md.members.contains(member) => {
                Err(Fail::new(name, format!("`{name}` is private to module {module} (mark it `pub` to export it)")))
            }
            Some(_) => Err(Fail::new(name, format!("module {module} has no member `{member}`"))),
            None => Err(Fail::new(name, format!("unknown module `{module}` in `{name}`; import it first"))),
        }
    }

    /// `head(args)` with the binding forms of the syntax handled.
    fn app(&mut self, t: T<'h>, head: T<'h>, args: &'h [T<'h>]) -> R<'h> {
        let env = self.env;
        let (st, hs) = (env.store, env.heads);
        let Some((h, _)) = hs.classify(st, t) else {
            // `failure(tag, ...)`: a failure tag is a name, not a definition of the module (decision F.12)
            if as_sym(st, head) == Some(self.failure) {
                if let Some(&tag) = args.first() {
                    if as_sym(st, tag).is_some() {
                        let rest = self.all(&args[1..])?;
                        return Ok(self.rebuild(t, head, args, [tag].into_iter().chain(rest).collect()));
                    }
                }
            }
            return self.generic(t, head, args);
        };
        match (h, args) {
            // pattern variable names are always local to their pattern
            (Head::Any | Head::Seq1 | Head::Seq0, _) => Ok(t),
            (Head::Named, &[x, p]) => {
                let p = self.term(p)?;
                Ok(self.rebuild(t, head, args, vec![x, p]))
            }
            // types are not names of the program (decision F.13)
            (Head::Typed, &[p, ty]) => {
                let p = self.term(p)?;
                Ok(self.rebuild(t, head, args, vec![p, ty]))
            }
            (Head::FunType | Head::TypeUnion | Head::TypeWhere, _) => Ok(t),
            // option names `f(x, name: v)` are names of the function's options, not symbols
            (Head::Options, _) => {
                let mut out = Vec::with_capacity(args.len());
                for &a in args {
                    out.push(match hs.is(st, a, Head::Rule, 2) {
                        Some(&[k, v]) if as_sym(st, k).is_some() => {
                            let v2 = self.term(v)?;
                            if v2 == v {
                                a
                            } else {
                                st.call(hs.sym(Head::Rule), &[k, v2])
                            }
                        }
                        _ => self.term(a)?,
                    });
                }
                Ok(self.rebuild(t, head, args, out))
            }
            (Head::Fn, &[params, body]) => {
                let Some(ps) = list_args(st, hs, params) else {
                    return self.generic(t, head, args);
                };
                self.scopes.push_lets(lambda_param_names(st, hs, ps));
                let r = self.lambda(params, ps, body);
                self.scopes.pop();
                let (params2, body2) = r?;
                Ok(self.rebuild(t, head, args, vec![params2, body2]))
            }
            (Head::Block, stmts) => {
                self.scopes.push();
                let r = self.block(stmts);
                self.scopes.pop();
                Ok(self.rebuild(t, head, args, r?))
            }
            (Head::For, &[x, it, body]) => {
                let it = self.term(it)?;
                self.scopes.push_lets(as_sym(st, x));
                let body = self.term(body);
                self.scopes.pop();
                Ok(self.rebuild(t, head, args, vec![x, it, body?]))
            }
            (Head::Rule | Head::RuleDelayed, &[l, r]) => self.binder(t, head, args, l, &[r]),
            (Head::Set | Head::SetDelayed, &[l, v]) if matches!(st.view(l), View::App { .. }) => {
                self.binder(t, head, args, l, &[v])
            }
            (Head::DefineFn, &[lhs, body, opts, result, decs]) => {
                let mut vars = pattern_vars(st, hs, lhs);
                vars.extend(pattern_vars(st, hs, opts));
                self.scopes.push_lets(vars);
                let r = self.all(&[lhs, body, opts]);
                self.scopes.pop();
                let mut out = r?;
                out.push(result);
                out.push(self.decorators(decs)?);
                Ok(self.rebuild(t, head, args, out))
            }
            (Head::DefineRule, &[rule, decs]) => {
                let out = vec![self.term(rule)?, self.decorators(decs)?];
                Ok(self.rebuild(t, head, args, out))
            }
            (Head::DefineSym, &[s, decs]) => {
                let out = vec![self.term(s)?, self.decorators(decs)?];
                Ok(self.rebuild(t, head, args, out))
            }
            (Head::Module | Head::Import | Head::ImportExcept | Head::Public, _) => Err(Fail::new(
                h.name(),
                "`module`, `import` and `pub` are statements: only allowed at the top level (and `pub` and `import` in a module body)",
            )),
            _ => self.generic(t, head, args),
        }
    }

    /// A form binding the pattern variables of `pattern` in itself and in `rest`: rules, compound definitions.
    fn binder(&mut self, t: T<'h>, head: T<'h>, args: &'h [T<'h>], pattern: T<'h>, rest: &[T<'h>]) -> R<'h> {
        let env = self.env;
        self.scopes.push_lets(pattern_vars(env.store, env.heads, pattern));
        let r = (|| {
            let mut out = vec![self.term(pattern)?];
            out.extend(self.all(rest)?);
            Ok(out)
        })();
        self.scopes.pop();
        Ok(self.rebuild(t, head, args, r?))
    }

    /// A lambda's parameter list (defaults are resolved; names, types and wrappers are kept) and body, with the
    /// parameters bound.
    fn lambda(&mut self, params: T<'h>, ps: &'h [T<'h>], body: T<'h>) -> Result<(T<'h>, T<'h>), Fail> {
        let env = self.env;
        let mut out = Vec::with_capacity(ps.len());
        for &p in ps {
            out.push(self.param(p)?);
        }
        let list = env.heads.sym(Head::List);
        let params2 = if out.as_slice() == ps { params } else { env.store.call(list, &out) };
        Ok((params2, self.term(body)?))
    }

    /// One lambda parameter: `x`, `typed(x, T)`, `rest(x)`, `held(p)`, `optional(p, default)`.
    fn param(&mut self, p: T<'h>) -> R<'h> {
        let env = self.env;
        let (st, hs) = (env.store, env.heads);
        match (hs.classify(st, p), st.view(p)) {
            (Some((Head::Held, &[q])), View::App { head, args }) => {
                let q2 = self.param(q)?;
                Ok(self.rebuild(p, head, args, vec![q2]))
            }
            (Some((Head::Optional, &[q, d])), View::App { head, args }) => {
                let q2 = self.param(q)?;
                let d2 = self.term(d)?;
                Ok(self.rebuild(p, head, args, vec![q2, d2]))
            }
            _ => Ok(p),
        }
    }

    /// The statements of a block: `let`/`var` bind their name for the statements after them.
    fn block(&mut self, stmts: &'h [T<'h>]) -> Result<Vec<T<'h>>, Fail> {
        let env = self.env;
        let (st, hs) = (env.store, env.heads);
        let mut out = Vec::with_capacity(stmts.len());
        for &s in stmts {
            match (statement_binding(st, hs, s), st.view(s)) {
                (Some((x, b)), View::App { head, args: a @ &[lhs, v] }) => {
                    let v2 = self.term(v)?;
                    out.push(self.rebuild(s, head, a, vec![lhs, v2]));
                    self.scopes.bind(x, b);
                }
                _ => out.push(self.term(s)?),
            }
        }
        Ok(out)
    }

    /// A decorator list: decorator names are kept, their arguments (`on(s)`, `identity(e)`) resolved.
    fn decorators(&mut self, decs: T<'h>) -> R<'h> {
        let st = self.env.store;
        let View::App { head, args } = st.view(decs) else { return Ok(decs) };
        let mut out = Vec::with_capacity(args.len());
        for &d in args {
            out.push(match st.view(d) {
                View::App { head: dh, args: da } => {
                    let a = self.all(da)?;
                    self.rebuild(d, dh, da, a)
                }
                _ => d,
            });
        }
        Ok(self.rebuild(decs, head, args, out))
    }

    /// Any other call: head and arguments resolved.
    fn generic(&mut self, t: T<'h>, head: T<'h>, args: &'h [T<'h>]) -> R<'h> {
        let h2 = self.term(head)?;
        let a2 = self.all(args)?;
        if h2 == head && a2.as_slice() == args {
            return Ok(t);
        }
        Ok(self.env.store.app(h2, &a2))
    }

    fn all(&mut self, ts: &[T<'h>]) -> Result<Vec<T<'h>>, Fail> {
        ts.iter().map(|&x| self.term(x)).collect()
    }

    /// `head(new)`, or `t` itself when nothing changed (so unchanged statements keep their terms).
    fn rebuild(&self, t: T<'h>, head: T<'h>, old: &[T<'h>], new: Vec<T<'h>>) -> T<'h> {
        if new.as_slice() == old {
            t
        } else {
            self.env.store.app(head, &new)
        }
    }
}

/// The arguments of `list(...)`.
fn list_args<'h, S: Store + ?Sized>(st: &'h S, hs: &Heads, t: T<'h>) -> Option<&'h [T<'h>]> {
    match hs.classify(st, t) {
        Some((Head::List, args)) => Some(args),
        _ => None,
    }
}

/// The name a definition statement defines (decision F.2): `fn f`, `rule f(...)`, `sym s`, `x = v`, `f(x) := e`,
/// top-level `let x`/`var x`.
pub(crate) fn defined_name<'h, S: Store + ?Sized>(st: &'h S, hs: &Heads, t: T<'h>) -> Option<Sym> {
    let (h, args) = hs.classify(st, t)?;
    match (h, args) {
        (Head::DefineFn, [lhs, ..]) => head_sym(st, hs, *lhs),
        (Head::DefineRule, [rule, _]) => match hs.classify(st, *rule) {
            Some((Head::Rule | Head::RuleDelayed, [l, _])) => head_sym(st, hs, *l),
            _ => None,
        },
        (Head::DefineSym, [s, _]) => as_sym(st, *s),
        (Head::Set | Head::SetDelayed, [l, _]) => as_sym(st, *l).or_else(|| head_sym(st, hs, *l)),
        (Head::Let | Head::Var, [lhs, _]) => match hs.is(st, *lhs, Head::Typed, 2) {
            Some(ta) => as_sym(st, ta[0]),
            None => as_sym(st, *lhs),
        },
        _ => None,
    }
}

/// The head symbol of a definition's left side, under any `condition(...)` wrappers.
fn head_sym<'h, S: Store + ?Sized>(st: &'h S, hs: &Heads, mut t: T<'h>) -> Option<Sym> {
    // a few levels at most: one per `when`
    for _ in 0..8 {
        match hs.is(st, t, Head::Condition, 2) {
            Some(a) => t = a[0],
            None => break,
        }
    }
    match st.view(t) {
        View::App { head, .. } => as_sym(st, head),
        _ => None,
    }
}
