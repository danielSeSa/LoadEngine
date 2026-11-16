use std::collections::{HashMap, HashSet};

use glam::Vec3;

use super::mesh::{key, EditMesh, NONE};

struct Dsu {
    p: Vec<u32>,
}

impl Dsu {
    fn new(n: usize) -> Self {
        Self { p: (0..n as u32).collect() }
    }
    fn find(&mut self, mut x: u32) -> u32 {
        while self.p[x as usize] != x {
            let g = self.p[self.p[x as usize] as usize];
            self.p[x as usize] = g;
            x = g;
        }
        x
    }
    fn union(&mut self, a: u32, b: u32) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.p[ra as usize] = rb;
        }
    }
}

impl EditMesh {
    // ------------------------------------------------------------
    //  Selection
    // ------------------------------------------------------------

    pub fn select_linked(&mut self) -> bool {
        if self.sel_v.is_empty() {
            return false;
        }
        let mut d = Dsu::new(self.verts.len());
        for f in &self.faces {
            for w in f.windows(2) {
                d.union(w[0], w[1]);
            }
        }
        let mut roots: HashSet<u32> = HashSet::new();
        for &v in &self.sel_v {
            roots.insert(d.find(v));
        }
        for v in 0..self.verts.len() as u32 {
            if roots.contains(&d.find(v)) {
                self.sel_v.insert(v);
            }
        }
        self.flush_from_verts();
        true
    }

    pub fn select_more(&mut self) -> bool {
        if self.sel_v.is_empty() {
            return false;
        }
        let mut add: Vec<u32> = Vec::new();
        for f in &self.faces {
            if f.iter().any(|v| self.sel_v.contains(v)) {
                add.extend(f.iter().copied());
            }
        }
        self.sel_v.extend(add);
        self.flush_from_verts();
        true
    }

    pub fn select_less(&mut self) -> bool {
        if self.sel_v.is_empty() {
            return false;
        }
        let mut rm: HashSet<u32> = HashSet::new();
        for e in &self.edges {
            let sa = self.sel_v.contains(&e.a);
            let sb = self.sel_v.contains(&e.b);
            if sa != sb {
                rm.insert(if sa { e.a } else { e.b });
            }
        }
        for v in rm {
            self.sel_v.remove(&v);
        }
        self.flush_from_verts();
        true
    }

    pub fn invert_verts(&mut self) {
        let n = self.verts.len() as u32;
        let old = std::mem::take(&mut self.sel_v);
        self.sel_v = (0..n).filter(|v| !old.contains(v)).collect();
        self.flush_from_verts();
    }

    pub fn invert_faces(&mut self) {
        let n = self.faces.len() as u32;
        let old = std::mem::take(&mut self.sel_f);
        self.sel_f = (0..n).filter(|f| !old.contains(f)).collect();
        self.flush_from_faces();
    }

    fn vert_edges(&self) -> Vec<Vec<u32>> {
        let mut v: Vec<Vec<u32>> = vec![Vec::new(); self.verts.len()];
        for (i, e) in self.edges.iter().enumerate() {
            v[e.a as usize].push(i as u32);
            v[e.b as usize].push(i as u32);
        }
        v
    }

    fn loop_next(&self, ve: &[Vec<u32>], cur: usize, at: u32) -> Option<usize> {
        let e = self.edges[cur];
        let other = if e.a == at { e.b } else { e.a };
        let inc = &ve[at as usize];

        if e.f1 == NONE {
            let c: Vec<usize> = inc
                .iter()
                .map(|&i| i as usize)
                .filter(|&i| i != cur && self.edges[i].f1 == NONE)
                .collect();
            return if c.len() == 1 { Some(c[0]) } else { None };
        }
        if inc.len() != 4 {
            return None;
        }
        let mut excl: HashSet<u32> = HashSet::new();
        excl.insert(other);
        for fi in [e.f0, e.f1] {
            let f = &self.faces[fi as usize];
            let n = f.len();
            if let Some(p) = f.iter().position(|&v| v == at) {
                excl.insert(f[(p + 1) % n]);
                excl.insert(f[(p + n - 1) % n]);
            }
        }
        let c: Vec<usize> = inc
            .iter()
            .map(|&i| i as usize)
            .filter(|&i| {
                let ee = self.edges[i];
                let o = if ee.a == at { ee.b } else { ee.a };
                i != cur && !excl.contains(&o)
            })
            .collect();
        if c.len() == 1 { Some(c[0]) } else { None }
    }

    /// رأس‌های Edge Loop گذرنده از یال start
    pub fn edge_loop(&self, start: usize) -> Vec<u32> {
        let ve = self.vert_edges();
        let mut seen: HashSet<usize> = HashSet::new();
        let mut out: HashSet<u32> = HashSet::new();
        let e0 = self.edges[start];
        seen.insert(start);
        out.insert(e0.a);
        out.insert(e0.b);
        for dir in 0..2 {
            let mut cur = start;
            let mut at = if dir == 0 { e0.b } else { e0.a };
            let mut guard = 0;
            while guard < self.edges.len() {
                guard += 1;
                let Some(next) = self.loop_next(&ve, cur, at) else { break };
                if !seen.insert(next) {
                    break;
                }
                let e = self.edges[next];
                out.insert(e.a);
                out.insert(e.b);
                at = if e.a == at { e.b } else { e.a };
                cur = next;
            }
        }
        out.into_iter().collect()
    }

    // ------------------------------------------------------------
    //  Face tools
    // ------------------------------------------------------------

    /// ساخت یک وجه از رأس‌های انتخاب‌شده (مرتب‌سازی زاویه‌ای)
    pub fn fill(&mut self) -> bool {
        let mut ids: Vec<u32> = self.sel_v.iter().copied().collect();
        ids.sort_unstable();
        if ids.len() < 3 || ids.len() > 512 {
            return false;
        }
        let pts: Vec<Vec3> = ids.iter().map(|&i| self.verts[i as usize]).collect();
        let c = pts.iter().copied().sum::<Vec3>() / pts.len() as f32;

        let mut i0 = 0;
        let mut d0 = 0.0f32;
        for (i, p) in pts.iter().enumerate() {
            let d = (*p - c).length_squared();
            if d > d0 {
                d0 = d;
                i0 = i;
            }
        }
        if d0 < 1e-12 {
            return false;
        }
        let u = (pts[i0] - c).normalize();
        let mut n = Vec3::ZERO;
        let mut best = 0.0f32;
        for p in &pts {
            let cr = u.cross(*p - c);
            let l = cr.length_squared();
            if l > best {
                best = l;
                n = cr;
            }
        }
        if best < 1e-12 {
            return false;
        }
        let n = n.normalize();
        let w = n.cross(u);

        let mut order: Vec<(f32, u32)> = ids
            .iter()
            .zip(pts.iter())
            .map(|(&i, p)| {
                let d = *p - c;
                (d.dot(w).atan2(d.dot(u)), i)
            })
            .collect();
        order.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let face: Vec<u32> = order.into_iter().map(|o| o.1).collect();

        self.faces.push(face);
        let fi = self.faces.len() as u32 - 1;
        self.rebuild_edges();
        self.recalc_outside();
        self.sel_f.clear();
        self.sel_f.insert(fi);
        self.flush_from_faces();
        true
    }

    /// ادغام دو وجهِ دو طرف هر یال انتخاب‌شده
    pub fn dissolve_edges(&mut self) -> bool {
        let nf = self.faces.len();
        let mut d = Dsu::new(nf);
        let mut any = false;
        for e in &self.edges {
            if e.f1 != NONE && self.sel_v.contains(&e.a) && self.sel_v.contains(&e.b) {
                d.union(e.f0, e.f1);
                any = true;
            }
        }
        if !any {
            return false;
        }
        let mut groups: HashMap<u32, Vec<u32>> = HashMap::new();
        for f in 0..nf as u32 {
            let r = d.find(f);
            groups.entry(r).or_default().push(f);
        }

        let mut remove: HashSet<u32> = HashSet::new();
        let mut add: Vec<Vec<u32>> = Vec::new();
        for (_, g) in groups {
            if g.len() < 2 {
                continue;
            }
            let mut dir: HashSet<(u32, u32)> = HashSet::new();
            for &fi in &g {
                let f = &self.faces[fi as usize];
                let n = f.len();
                for i in 0..n {
                    dir.insert((f[i], f[(i + 1) % n]));
                }
            }
            let mut next: HashMap<u32, u32> = HashMap::new();
            let mut ok = true;
            let mut count = 0usize;
            for &(a, b) in &dir {
                if dir.contains(&(b, a)) {
                    continue;
                }
                if next.insert(a, b).is_some() {
                    ok = false;
                    break;
                }
                count += 1;
            }
            if !ok || count < 3 {
                continue;
            }
            let start = *next.keys().next().unwrap();
            let mut poly = vec![start];
            let mut cur = next[&start];
            while cur != start && poly.len() <= count {
                poly.push(cur);
                match next.get(&cur) {
                    Some(&n) => cur = n,
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok || cur != start || poly.len() != count {
                continue;
            }
            remove.extend(g.iter().copied());
            add.push(poly);
        }
        if add.is_empty() {
            return false;
        }
        let old = std::mem::take(&mut self.faces);
        for (i, f) in old.into_iter().enumerate() {
            if !remove.contains(&(i as u32)) {
                self.faces.push(f);
            }
        }
        self.faces.extend(add);
        self.compact();
        true
    }

    /// هر وجه انتخاب‌شده به مثلث‌هایی به سمت مرکز تبدیل می‌شود؛ مراکز انتخاب می‌شوند
    pub fn poke(&mut self) -> bool {
        if self.sel_f.is_empty() {
            return false;
        }
        let old = std::mem::take(&mut self.faces);
        let mut centers: Vec<u32> = Vec::new();
        for (i, f) in old.into_iter().enumerate() {
            if self.sel_f.contains(&(i as u32)) {
                let c = f.iter().map(|&v| self.verts[v as usize]).sum::<Vec3>() / f.len() as f32;
                let ci = self.verts.len() as u32;
                self.verts.push(c);
                centers.push(ci);
                let n = f.len();
                for k in 0..n {
                    self.faces.push(vec![f[k], f[(k + 1) % n], ci]);
                }
            } else {
                self.faces.push(f);
            }
        }
        self.sel_f.clear();
        self.sel_v = centers.into_iter().collect();
        self.rebuild_edges();
        self.flush_from_verts();
        true
    }

    /// Solidify: دیواره‌ای به ضخامت t (به سمت داخل)
    pub fn solidify(&mut self, t: f32) -> bool {
        if self.faces.is_empty() || self.verts.len() > 150_000 {
            return false;
        }
        self.recalc_outside();
        let nv = self.verts.len();
        let mut vn = vec![Vec3::ZERO; nv];
        for fi in 0..self.faces.len() {
            let n = self.face_normal(fi);
            for &v in &self.faces[fi] {
                vn[v as usize] += n;
            }
        }
        let boundary: HashSet<(u32, u32)> = self
            .edges
            .iter()
            .filter(|e| e.f1 == NONE)
            .map(|e| (e.a, e.b))
            .collect();

        let off = nv as u32;
        for i in 0..nv {
            let p = self.verts[i] - vn[i].normalize_or_zero() * t;
            self.verts.push(p);
        }
        let old: Vec<Vec<u32>> = self.faces.clone();
        for f in &old {
            self.faces.push(f.iter().rev().map(|&v| v + off).collect());
        }
        for f in &old {
            let n = f.len();
            for i in 0..n {
                let (a, b) = (f[i], f[(i + 1) % n]);
                if boundary.contains(&key(a, b)) {
                    self.faces.push(vec![a, a + off, b + off, b]);
                }
            }
        }
        self.select_none();
        self.rebuild_edges();
        true
    }

    /// کپی وجه‌های انتخاب‌شده (کپی‌ها انتخاب می‌شوند)
    pub fn duplicate_faces(&mut self) -> bool {
        if self.sel_f.is_empty() {
            return false;
        }
        let mut order: Vec<u32> = self.sel_f.iter().copied().collect();
        order.sort_unstable();
        let mut map: HashMap<u32, u32> = HashMap::new();
        let mut newf: Vec<Vec<u32>> = Vec::new();
        for fi in order {
            let f = self.faces[fi as usize].clone();
            let mut g = Vec::with_capacity(f.len());
            for v in f {
                let x = if let Some(&x) = map.get(&v) {
                    x
                } else {
                    let x = self.verts.len() as u32;
                    let p = self.verts[v as usize];
                    self.verts.push(p);
                    map.insert(v, x);
                    x
                };
                g.push(x);
            }
            newf.push(g);
        }
        let start = self.faces.len() as u32;
        self.faces.extend(newf);
        self.sel_f = (start..self.faces.len() as u32).collect();
        self.flush_from_faces();
        self.rebuild_edges();
        true
    }

    /// Extrude برای یال‌های انتخاب‌شده (ساخت نوار وجه)
    pub fn extrude_edges(&mut self) -> bool {
        let sel: Vec<(u32, u32)> = self
            .edges
            .iter()
            .filter(|e| self.sel_v.contains(&e.a) && self.sel_v.contains(&e.b))
            .map(|e| (e.a, e.b))
            .collect();
        if sel.is_empty() {
            return false;
        }
        let mut vs: Vec<u32> = sel.iter().flat_map(|&(a, b)| [a, b]).collect();
        vs.sort_unstable();
        vs.dedup();
        let mut map: HashMap<u32, u32> = HashMap::new();
        for v in vs {
            let p = self.verts[v as usize];
            let n = self.verts.len() as u32;
            self.verts.push(p);
            map.insert(v, n);
        }
        for (a, b) in sel {
            self.faces.push(vec![a, b, map[&b], map[&a]]);
        }
        self.sel_v = map.values().copied().collect();
        self.rebuild_edges();
        self.recalc_outside();
        self.flush_from_verts();
        true
    }

    /// وجه‌های انتخاب‌شده را به مش جدید منتقل می‌کند
    pub fn separate_faces(&mut self) -> Option<EditMesh> {
        if self.sel_f.is_empty() || self.sel_f.len() >= self.faces.len() {
            return None;
        }
        let mut out = EditMesh::default();
        let mut map: HashMap<u32, u32> = HashMap::new();
        let old = std::mem::take(&mut self.faces);
        for (i, f) in old.into_iter().enumerate() {
            if self.sel_f.contains(&(i as u32)) {
                let mut g: Vec<u32> = Vec::with_capacity(f.len());
                for v in f {
                    let x = if let Some(&x) = map.get(&v) {
                        x
                    } else {
                        let x = out.verts.len() as u32;
                        out.verts.push(self.verts[v as usize]);
                        map.insert(v, x);
                        x
                    };
                    g.push(x);
                }
                out.faces.push(g);
            } else {
                self.faces.push(f);
            }
        }
        self.compact();
        out.rebuild_edges();
        Some(out)
    }

    // ------------------------------------------------------------
    //  Whole-mesh helpers
    // ------------------------------------------------------------

    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        if self.verts.is_empty() {
            return None;
        }
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for v in &self.verts {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
        Some((lo, hi))
    }

    pub fn recenter(&mut self) -> bool {
        let Some((lo, hi)) = self.bounds() else { return false };
        let c = (lo + hi) * 0.5;
        for v in self.verts.iter_mut() {
            *v -= c;
        }
        true
    }

    /// مبدأ به مرکز کف مش (مناسب برای قرار گرفتن روی زمین در بازی)
    pub fn origin_to_bottom(&mut self) -> bool {
        let Some((lo, hi)) = self.bounds() else { return false };
        let c = Vec3::new((lo.x + hi.x) * 0.5, lo.y, (lo.z + hi.z) * 0.5);
        for v in self.verts.iter_mut() {
            *v -= c;
        }
        true
    }

    pub fn to_sphere(&mut self, f: f32) -> bool {
        let ids: Vec<u32> = if self.sel_v.is_empty() {
            (0..self.verts.len() as u32).collect()
        } else {
            self.sel_v.iter().copied().collect()
        };
        if ids.len() < 2 {
            return false;
        }
        let c = ids.iter().map(|&i| self.verts[i as usize]).sum::<Vec3>() / ids.len() as f32;
        let r = ids.iter().map(|&i| (self.verts[i as usize] - c).length()).sum::<f32>()
            / ids.len() as f32;
        for &i in &ids {
            let p = self.verts[i as usize];
            let d = p - c;
            let l = d.length();
            if l > 1e-6 {
                self.verts[i as usize] = p.lerp(c + d / l * r, f);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::mesh::{cube, grid};

    #[test]
    fn dissolve_cube_edge() {
        let mut m = cube();
        let (a, b) = (m.edges[0].a, m.edges[0].b);
        m.sel_v.insert(a);
        m.sel_v.insert(b);
        assert!(m.dissolve_edges());
        assert_eq!(m.faces.len(), 5);
    }

    #[test]
    fn poke_one_face() {
        let mut m = cube();
        m.sel_f.insert(0);
        assert!(m.poke());
        assert_eq!(m.faces.len(), 9);
    }

    #[test]
    fn solidify_plane() {
        let mut m = grid(1);
        assert!(m.solidify(0.1));
        assert_eq!((m.verts.len(), m.faces.len()), (8, 6));
    }

    #[test]
    fn extrude_edge_makes_quad() {
        let mut m = grid(1);
        let (a, b) = (m.edges[0].a, m.edges[0].b);
        m.sel_v.insert(a);
        m.sel_v.insert(b);
        assert!(m.extrude_edges());
        assert_eq!(m.faces.len(), 2);
    }
}