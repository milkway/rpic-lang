//! `State` methods that resolve positions/places and evaluate
//! expression trees. Split out of the eval core (#323).

use super::*;

impl State {
    // ---- positions & places ------------------------------------------------

    pub(super) fn eval_pos(&mut self, pos: &Position) -> ER<Point> {
        match pos {
            Position::Pair(x, y) => Ok(Point::new(self.expr_dim(x)?, self.expr_dim(y)?)),
            Position::Place(loc) => self.eval_loc(loc),
            Position::Between { frac, a, b, .. } => {
                let f = self.eval_expr(frac)?;
                let pa = self.eval_pos(a)?;
                let pb = self.eval_pos(b)?;
                Ok(pa.lerp(pb, f))
            }
            Position::Sum(sign, a, b) => {
                let pa = self.eval_pos(a)?;
                let pb = self.eval_pos(b)?;
                Ok(match sign {
                    Sign::Plus => pa + pb,
                    Sign::Minus => pa - pb,
                })
            }
            Position::Scale(p, s, div) => {
                let pp = self.eval_pos(p)?;
                let sv = self.eval_expr(s)?;
                if *div {
                    if sv == 0.0 {
                        return err("division by zero in position");
                    }
                    Ok(pp / sv)
                } else {
                    Ok(pp * sv)
                }
            }
        }
    }

    fn eval_loc(&mut self, loc: &Location) -> ER<Point> {
        match loc {
            Location::Place(p) => self.place_point(p),
            Location::Paren(pos) => self.eval_pos(pos),
            Location::ParenPair(p1, p2) => {
                Ok(Point::new(self.eval_pos(p1)?.x, self.eval_pos(p2)?.y))
            }
        }
    }

    pub(super) fn place_point(&mut self, p: &Place) -> ER<Point> {
        match p {
            Place::Here => Ok(self.pos),
            Place::Name {
                name,
                subscript,
                span,
            } => {
                let key = self.indexed_name(name, subscript.as_deref())?;
                Ok(self.resolve_label(&key, span.as_ref())?.center)
            }
            Place::Nth { count, obj, span } => {
                let idx = self.nth_index(count, obj, span.as_ref())?;
                Ok(self.placed[idx].center)
            }
            Place::Corner(inner, c) => {
                let pl = self.resolve_obj(inner)?;
                Ok(pl.corner(*c))
            }
            Place::CornerOf(c, inner) => {
                let pl = self.resolve_obj(inner)?;
                Ok(pl.corner(*c))
            }
            Place::Member(..) => Ok(self.resolve_obj(p)?.center),
        }
    }

    /// Resolve an object-valued place to its [`Placed`] record, descending into
    /// block members for `B.A` / `last [].Outer`.
    pub(super) fn resolve_obj(&mut self, p: &Place) -> ER<Placed> {
        match p {
            Place::Name {
                name,
                subscript,
                span,
            } => {
                let key = self.indexed_name(name, subscript.as_deref())?;
                self.resolve_label(&key, span.as_ref())
            }
            Place::Nth { count, obj, span } => {
                let idx = self.nth_index(count, obj, span.as_ref())?;
                Ok(self.placed[idx].clone())
            }
            Place::Corner(inner, _) | Place::CornerOf(_, inner) => self.resolve_obj(inner),
            Place::Member(base, sub) => {
                let b = self.resolve_obj(base)?;
                let key = match sub.as_ref() {
                    Place::Name {
                        name, subscript, ..
                    } => self.indexed_name(name, subscript.as_deref())?,
                    _ => return err("a block sub-label must be a name"),
                };
                b.members.get(&key).cloned().ok_or_else(|| EvalError {
                    msg: format!("no sub-label `{key}` in that block"),
                    info: None,
                })
            }
            Place::Here => err("`Here` is a point, not an object"),
        }
    }

    pub(super) fn place_index(&mut self, p: &Place) -> ER<usize> {
        match p {
            Place::Name {
                name,
                subscript,
                span,
            } => {
                let key = self.indexed_name(name, subscript.as_deref())?;
                self.label_index(&key, span.as_ref())
            }
            Place::Nth { count, obj, span } => self.nth_index(count, obj, span.as_ref()),
            Place::Corner(inner, _) | Place::CornerOf(_, inner) => self.place_index(inner),
            Place::Here => err("`Here` is a point, not an object"),
            Place::Member(_, _) => err("block sub-labels (B.A) are not supported yet"),
        }
    }

    fn label_index(&self, name: &str, span: Option<&Span>) -> ER<usize> {
        self.labels
            .get(name)
            .copied()
            .ok_or_else(|| unknown_label_error(name, span))
    }

    /// Resolve a label to its [`Placed`], falling back to enclosing-scope labels
    /// (absolute coordinates) so a block can reference outer labels.
    fn resolve_label(&self, key: &str, span: Option<&Span>) -> ER<Placed> {
        if let Some(&idx) = self.labels.get(key) {
            Ok(self.placed[idx].clone())
        } else if let Some(pl) = self.outer_labels.get(key) {
            Ok(pl.clone())
        } else {
            Err(unknown_label_error(key, span))
        }
    }

    pub(super) fn label_key(&mut self, label: &Label) -> ER<String> {
        self.indexed_name(&label.name, label.subscript.as_ref())
    }

    pub(super) fn indexed_name(&mut self, name: &str, subscript: Option<&Expr>) -> ER<String> {
        match subscript {
            Some(Expr::Index(items)) => {
                let mut parts = Vec::with_capacity(items.len());
                for e in items {
                    parts.push(fmt_num(self.eval_expr(e)?));
                }
                Ok(format!("{name}[{}]", parts.join(",")))
            }
            Some(e) => Ok(format!("{name}[{}]", fmt_num(self.eval_expr(e)?))),
            None => Ok(name.to_string()),
        }
    }

    fn nth_index(&mut self, count: &Nth, obj: &PrimObj, span: Option<&Span>) -> ER<usize> {
        // Untyped `last` matches the most recent object of any kind; a typed
        // reference (`last box`) filters to that kind.
        let want = primobj_kind(obj);
        let matches: Vec<usize> = self
            .placed
            .iter()
            .enumerate()
            .filter(|(_, pl)| want.is_none_or(|w| pl.kind == w))
            .map(|(i, _)| i)
            .collect();
        if matches.is_empty() {
            return match want {
                Some(w) => err(format!("no {:?} object to reference", want_name(w))),
                None => err("no object to reference".to_string()),
            };
        }
        let idx = match count {
            Nth::Last => *matches.last().unwrap(),
            Nth::Count(e, from_last) => {
                let n = self.eval_expr(e)?.round() as i64;
                if n < 1 {
                    return err("ordinal must be >= 1");
                }
                let k = (n - 1) as usize;
                if *from_last {
                    if k >= matches.len() {
                        return err_diag(ordinal_diagnostic(n, matches.len(), span));
                    }
                    matches[matches.len() - 1 - k]
                } else {
                    if k >= matches.len() {
                        return err_diag(ordinal_diagnostic(n, matches.len(), span));
                    }
                    matches[k]
                }
            }
        };
        Ok(idx)
    }

    // ---- string expressions ------------------------------------------------

    pub(super) fn eval_stringexpr(&mut self, se: &StringExpr) -> ER<String> {
        Ok(match se {
            StringExpr::Lit(s) => s.clone(),
            StringExpr::Concat(a, b) => {
                format!("{}{}", self.eval_stringexpr(a)?, self.eval_stringexpr(b)?)
            }
            StringExpr::Arg(n) => format!("${n}"), // should have been expanded
            StringExpr::Sprintf(fmt, args) => {
                let f = self.eval_stringexpr(fmt)?;
                let mut vals = Vec::with_capacity(args.len());
                for e in args {
                    vals.push(self.eval_printf_arg(e)?);
                }
                sprintf_fmt(&f, &vals)
            }
            StringExpr::SvgFont(_) => String::new(),
            StringExpr::Rgb(args) => {
                let mut c = [0u32; 3];
                for (i, e) in args.iter().enumerate() {
                    let v = self.eval_expr(e)?;
                    if !v.is_finite() || !(0.0..=255.0).contains(&v.round()) {
                        return err("rgb() component out of range 0-255");
                    }
                    c[i] = v.round() as u32;
                }
                format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
            }
            StringExpr::ColorNum(e) => {
                let v = self.eval_expr(e)?;
                num_to_color(v)?
            }
        })
    }

    fn eval_printf_arg(&mut self, e: &Expr) -> ER<PrintfArg> {
        match e {
            Expr::Str(se) => Ok(PrintfArg::Str(self.eval_stringexpr(se)?)),
            _ => Ok(PrintfArg::Num(self.eval_expr(e)?)),
        }
    }

    // ---- expressions -------------------------------------------------------

    /// Evaluate an operand to its string form (for `==`/`!=`); a numeric operand
    /// is rendered textually so `"$1" == "2"` style comparisons work uniformly.
    fn expr_str(&mut self, e: &Expr) -> ER<String> {
        match e {
            Expr::Str(se) => self.eval_stringexpr(se),
            _ => Ok(fmt_num(self.eval_expr(e)?)),
        }
    }

    pub(super) fn eval_expr(&mut self, e: &Expr) -> ER<f64> {
        let value = match e {
            Expr::Num(v) => *v,
            Expr::Str(_) => return err("a string is only valid as an `==`/`!=` operand"),
            Expr::Index(_) => return err("a comma subscript is only valid inside `name[...]`"),
            Expr::Var(name, subscript) => {
                let key = self.indexed_name(name, subscript.as_deref())?;
                self.vars.get(&key).copied().ok_or_else(|| EvalError {
                    msg: format!("variable not found `{key}`"),
                    info: None,
                })?
            }
            Expr::Env(v) => self.env.get(*v),
            Expr::Unary(op, a) => {
                let x = self.eval_expr(a)?;
                match op {
                    UnOp::Neg => -x,
                    UnOp::Pos => x,
                    UnOp::Not => bool_f(x == 0.0),
                }
            }
            Expr::Bin(..) => self.eval_bin_expr(e)?,
            Expr::Func1(f, a) => {
                let x = self.eval_expr(a)?;
                match f {
                    Func1::Abs => x.abs(),
                    Func1::Acos => x.acos(),
                    Func1::Asin => x.asin(),
                    Func1::Cos => x.cos(),
                    Func1::Exp => 10f64.powf(x),
                    Func1::Expe => x.exp(),
                    Func1::Int => x.trunc(),
                    Func1::Log => x.log10(),
                    Func1::Loge => x.ln(),
                    Func1::Sign => {
                        if x >= 0.0 {
                            1.0
                        } else {
                            -1.0
                        }
                    }
                    Func1::Sin => x.sin(),
                    Func1::Sqrt => x.sqrt(),
                    Func1::Tan => x.tan(),
                    Func1::Floor => x.floor(),
                }
            }
            Expr::Func2(f, a, b) => {
                let x = self.eval_expr(a)?;
                let y = self.eval_expr(b)?;
                match f {
                    Func2::Atan2 => x.atan2(y),
                    Func2::Max => x.max(y),
                    Func2::Min => x.min(y),
                    Func2::Pmod => x.rem_euclid(y),
                }
            }
            Expr::Rand(seed) => {
                let seed = match seed {
                    Some(e) => Some(self.eval_expr(e)?),
                    None => None,
                };
                self.rand(seed)
            }
            Expr::Assign(name, subscript, v) => {
                let val = self.eval_expr(v)?;
                let key = self.indexed_name(name, subscript.as_deref())?;
                self.vars.insert(key, val);
                val
            }
            Expr::DotX(loc) => {
                let x = self.eval_loc(loc)?.x;
                self.to_user_dim(x)
            }
            Expr::DotY(loc) => {
                let y = self.eval_loc(loc)?.y;
                self.to_user_dim(y)
            }
            Expr::PlaceAttr(place, param) => {
                let pl = self.resolve_obj(place)?;
                match param {
                    token::Param::Width => self.to_user_dim(pl.attr_width()),
                    token::Param::Height => self.to_user_dim(pl.attr_height()),
                    token::Param::Radius => self.to_user_dim(pl.attr_radius()),
                    token::Param::Diameter => self.to_user_dim(pl.attr_diameter()),
                    token::Param::Length => self.to_user_dim(pl.attr_length()),
                    token::Param::Thickness => pl.thick,
                }
            }
        };
        finite(value, "numeric expression")
    }

    fn eval_bin_expr(&mut self, e: &Expr) -> ER<f64> {
        let mut chain = Vec::new();
        let mut left = e;
        while let Expr::Bin(op, a, b) = left {
            chain.push((*op, b.as_ref()));
            left = a.as_ref();
        }

        let mut left_expr = Some(left);
        let mut acc = 0.0;
        for (op, right) in chain.into_iter().rev() {
            let current_left = left_expr.take();
            let string_cmp = matches!(op, BinOp::Eq | BinOp::Ne)
                && (matches!(current_left, Some(Expr::Str(_))) || matches!(right, Expr::Str(_)));
            acc = if string_cmp {
                let sa = match current_left {
                    Some(left) => self.expr_str(left)?,
                    None => fmt_num(acc),
                };
                let sb = self.expr_str(right)?;
                bool_f(if matches!(op, BinOp::Eq) {
                    sa == sb
                } else {
                    sa != sb
                })
            } else {
                let x = match current_left {
                    Some(left) => self.eval_expr(left)?,
                    None => acc,
                };
                let y = self.eval_expr(right)?;
                eval_numeric_bin(op, x, y)?
            };
            acc = finite(acc, "numeric expression")?;
        }
        Ok(acc)
    }

    fn rand(&mut self, seed: Option<f64>) -> f64 {
        if let Some(seed) = seed {
            self.rng.seed(seed.trunc() as i64);
        }
        self.rng.next_f64()
    }
}
