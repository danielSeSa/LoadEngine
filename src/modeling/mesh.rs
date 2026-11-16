use glam::Vec3;
use std::collections::{HashMap, HashSet};
use std::f32::consts::{PI, TAU};

pub const NONE: u32 = u32::MAX;

/// یال یکتا؛ a < b و f1 == NONE یعنی یال مرزی
#[derive(Clone, Copy, Debug)]
pub struct Edge {
    pub a: u32,
    pub b: u32,
    pub f0: u32,
    pub f1: u32,
}

#[derive(Clone, Default)]
pub struct EditMesh {
    pub verts: Vec<Vec3>,
    pub faces: Vec<Vec<u32>>,
    pub edges: Vec<Edge>,
    pub sel_v: HashSet<u32>,
    pub sel_f: HashSet<u32>,
}

pub fn key(a: u32, b: u32) -> (u32, u32) {
    if a < b { (a, b) } else { (b, a) }
}

fn has_dir(f: &[u32], a: u32, b: u32) -> bool {
    let n = f.len();
    (0..n).any(|i| f[i] == a && f[(i + 1) % n] == b)
}

impl EditMesh {
    // ------------------------------------------------------------
    //  Topology cache
    // ------------------------------------------------------------

    pub fn rebuild_edges(&mut self) {
        let mut map: HashMap<(u32, u32), usize> = HashMap::with_capacity(self.faces.len() * 2);
        self.edges.clear();
        for (fi, f) in self.faces.iter().enumerate() {
            let n = f.len();
            for i in 0..n {
                let k = key(f[i], f[(i + 1) % n]);
                match map.get(&k) {
                    Some(&ei) => {
                        let e = &mut self.edges[ei];
                        if e.f1 == NONE {
                            e.f1 = fi as u32;
                        }
                    }
                    None => {
                        map.insert(k, self.edges.len());
                        self.edges.push(Edge { a: k.0, b: k.1, f0: fi as u32, f1: NONE });
                    }
                }
            }
        }
    }

    pub fn edge_map(&self) -> HashMap<(u32, u32), usize> {
        self.edges.iter().enumerate().map(|(i, e)| ((e.a, e.b), i)).collect()
    }

    pub fn tri_count(&self) -> usize {
        self.faces.iter().map(|f| f.len().saturating_sub(2)).sum()
    }

    // ------------------------------------------------------------
    //  Geometry queries
    // ------------------------------------------------------------

    pub fn face_normal(&self, fi: usize) -> Vec3 {
        let f = &self.faces[fi];
        let mut n = Vec3::ZERO;
        for i in 0..f.len() {
            let a = self.verts[f[i] as usize];
            let b = self.verts[f[(i + 1) % f.len()] as usize];
            n += Vec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y));
        }
        n.normalize_or_zero()
    }

    pub fn face_center(&self, fi: usize) -> Vec3 {
        let f = &self.faces[fi];
        let s: Vec3 = f.iter().map(|&v| self.verts[v as usize]).sum();
        s / f.len().max(1) as f32
    }

    pub fn median(&self) -> Vec3 {
        if self.sel_v.is_empty() {
            return Vec3::ZERO;
        }
        let s: Vec3 = self.sel_v.iter().map(|&v| self.verts[v as usize]).sum();
        s / self.sel_v.len() as f32
    }

    pub fn selection_normal(&self) -> Vec3 {
        let mut n = Vec3::ZERO;
        for &f in &self.sel_f {
            n += self.face_normal(f as usize);
        }
        let n = n.normalize_or_zero();
        if n == Vec3::ZERO { Vec3::Y } else { n }
    }

    // ------------------------------------------------------------
    //  Selection (sel_v و sel_f همیشه با هم سازگار نگه داشته می‌شوند)
    // ------------------------------------------------------------

    pub fn flush_from_verts(&mut self) {
        self.sel_f.clear();
        for (fi, f) in self.faces.iter().enumerate() {
            if !f.is_empty() && f.iter().all(|v| self.sel_v.contains(v)) {
                self.sel_f.insert(fi as u32);
            }
        }
    }

    pub fn flush_from_faces(&mut self) {
        self.sel_v.clear();
        for &fi in &self.sel_f {
            for &v in &self.faces[fi as usize] {
                self.sel_v.insert(v);
            }
        }
    }

    pub fn select_all(&mut self) {
        self.sel_v = (0..self.verts.len() as u32).collect();
        self.sel_f = (0..self.faces.len() as u32).collect();
    }

    pub fn select_none(&mut self) {
        self.sel_v.clear();
        self.sel_f.clear();
    }

    // ------------------------------------------------------------
    //  Cleanup
    // ------------------------------------------------------------

    /// رأس‌های بی‌استفاده حذف می‌شوند (انتخاب پاک می‌شود)
    pub fn compact(&mut self) {
        let mut map = vec![NONE; self.verts.len()];
        for f in &self.faces {
            for &v in f {
                map[v as usize] = 0;
            }
        }
        let mut nv = Vec::with_capacity(self.verts.len());
        for (i, m) in map.iter_mut().enumerate() {
            if *m != NONE {
                *m = nv.len() as u32;
                nv.push(self.verts[i]);
            }
        }
        for f in self.faces.iter_mut() {
            for v in f.iter_mut() {
                *v = map[*v as usize];
            }
        }
        self.verts = nv;
        self.sel_v.clear();
        self.sel_f.clear();
        self.rebuild_edges();
    }

    fn apply_remap(&mut self, remap: &[u32]) {
        let old = std::mem::take(&mut self.faces);
        for f in old {
            let mut g: Vec<u32> = Vec::with_capacity(f.len());
            for v in f {
                let m = remap[v as usize];
                if g.last() != Some(&m) {
                    g.push(m);
                }
            }
            while g.len() > 1 && g.first() == g.last() {
                g.pop();
            }
            if g.len() >= 3 {
                self.faces.push(g);
            }
        }
        self.compact();
    }

    pub fn append(&mut self, o: &EditMesh) {
        let off = self.verts.len() as u32;
        self.verts.extend_from_slice(&o.verts);
        for f in &o.faces {
            self.faces.push(f.iter().map(|v| v + off).collect());
        }
        self.rebuild_edges();
    }

    // ------------------------------------------------------------
    //  Delete
    // ------------------------------------------------------------

    pub fn delete_verts(&mut self) -> bool {
        if self.sel_v.is_empty() {
            return false;
        }
        let sel = std::mem::take(&mut self.sel_v);
        self.faces.retain(|f| !f.iter().any(|v| sel.contains(v)));
        self.compact();
        true
    }

    pub fn delete_edges(&mut self) -> bool {
        if self.sel_v.len() < 2 {
            return false;
        }
        let sel = std::mem::take(&mut self.sel_v);
        let before = self.faces.len();
        self.faces.retain(|f| {
            let n = f.len();
            !(0..n).any(|i| sel.contains(&f[i]) && sel.contains(&f[(i + 1) % n]))
        });
        let changed = self.faces.len() != before;
        self.compact();
        changed
    }

    pub fn delete_faces(&mut self) -> bool {
        if self.sel_f.is_empty() {
            return false;
        }
        let sel = std::mem::take(&mut self.sel_f);
        let mut i = 0u32;
        self.faces.retain(|_| {
            let keep = !sel.contains(&i);
            i += 1;
            keep
        });
        self.compact();
        true
    }

    // ------------------------------------------------------------
    //  Extrude / Inset
    // ------------------------------------------------------------

    pub fn extrude_faces(&mut self) -> bool {
        if self.sel_f.is_empty() {
            return false;
        }
        let mut order: Vec<u32> = self.sel_f.iter().copied().collect();
        order.sort_unstable();

        let mut count: HashMap<(u32, u32), u32> = HashMap::new();
        for &fi in &order {
            let f = &self.faces[fi as usize];
            for i in 0..f.len() {
                *count.entry(key(f[i], f[(i + 1) % f.len()])).or_insert(0) += 1;
            }
        }
        let mut sides: Vec<(u32, u32)> = Vec::new();
        let mut bverts: HashSet<u32> = HashSet::new();
        for &fi in &order {
            let f = &self.faces[fi as usize];
            for i in 0..f.len() {
                let (a, b) = (f[i], f[(i + 1) % f.len()]);
                if count[&key(a, b)] == 1 {
                    sides.push((a, b));
                    bverts.insert(a);
                    bverts.insert(b);
                }
            }
        }
        if sides.is_empty() {
            return false;
        }
        let mut bv: Vec<u32> = bverts.into_iter().collect();
        bv.sort_unstable();
        let mut map: HashMap<u32, u32> = HashMap::new();
        for v in bv {
            let nv = self.verts.len() as u32;
            let p = self.verts[v as usize];
            self.verts.push(p);
            map.insert(v, nv);
        }
        for &fi in &order {
            for v in self.faces[fi as usize].iter_mut() {
                if let Some(&n) = map.get(v) {
                    *v = n;
                }
            }
        }
        for (a, b) in sides {
            self.faces.push(vec![a, b, map[&b], map[&a]]);
        }
        self.flush_from_faces();
        self.rebuild_edges();
        true
    }

    /// Inset مستقل برای هر وجه؛ خروجی: (رأس داخلی، موقعیت لبه، مرکز وجه)
    pub fn inset_faces(&mut self) -> Vec<(u32, Vec3, Vec3)> {
        let mut out = Vec::new();
        let mut order: Vec<u32> = self.sel_f.iter().copied().collect();
        order.sort_unstable();
        for fi in order {
            let fi = fi as usize;
            let f = self.faces[fi].clone();
            let n = f.len();
            let c = self.face_center(fi);
            let base = self.verts.len() as u32;
            for &v in &f {
                let p = self.verts[v as usize];
                self.verts.push(p);
                out.push((self.verts.len() as u32 - 1, p, c));
            }
            for i in 0..n {
                let j = (i + 1) % n;
                self.faces.push(vec![f[i], f[j], base + j as u32, base + i as u32]);
            }
            self.faces[fi] = (0..n as u32).map(|i| base + i).collect();
        }
        if !out.is_empty() {
            self.flush_from_faces();
            self.rebuild_edges();
        }
        out
    }

    // ------------------------------------------------------------
    //  Loop cut (Edge Ring)
    // ------------------------------------------------------------

    pub fn loop_cut(&mut self, cuts: usize) -> bool {
        let cuts = cuts.clamp(1, 16);
        let emap = self.edge_map();
        let Some(start) = self
            .edges
            .iter()
            .find(|e| self.sel_v.contains(&e.a) && self.sel_v.contains(&e.b))
            .copied()
        else {
            return false;
        };

        let mut ring: HashSet<(u32, u32)> = HashSet::new();
        let mut visited: HashMap<u32, usize> = HashMap::new();
        ring.insert((start.a, start.b));
        let mut stack: Vec<(u32, (u32, u32))> = Vec::new();
        for f in [start.f0, start.f1] {
            if f != NONE {
                stack.push((f, (start.a, start.b)));
            }
        }
        while let Some((fi, (ea, eb))) = stack.pop() {
            if visited.contains_key(&fi) {
                continue;
            }
            let f = &self.faces[fi as usize];
            if f.len() != 4 {
                continue;
            }
            let Some(i) = (0..4).find(|&i| key(f[i], f[(i + 1) % 4]) == (ea, eb)) else {
                continue;
            };
            visited.insert(fi, i);
            let opp = key(f[(i + 2) % 4], f[(i + 3) % 4]);
            ring.insert(opp);
            let e = self.edges[emap[&opp]];
            let other = if e.f0 == fi { e.f1 } else { e.f0 };
            if other != NONE {
                stack.push((other, opp));
            }
        }

        // رأس‌های جدید روی هر یال حلقه، مرتب از min به max
        let mut newpts: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
        for &(a, b) in &ring {
            let (pa, pb) = (self.verts[a as usize], self.verts[b as usize]);
            let mut ids = Vec::with_capacity(cuts);
            for k in 1..=cuts {
                let t = k as f32 / (cuts + 1) as f32;
                ids.push(self.verts.len() as u32);
                self.verts.push(pa.lerp(pb, t));
            }
            newpts.insert((a, b), ids);
        }
        let orient = |from: u32, to: u32| -> Vec<u32> {
            let l = &newpts[&key(from, to)];
            if from < to { l.clone() } else { l.iter().rev().copied().collect() }
        };

        let old = std::mem::take(&mut self.faces);
        let mut out: Vec<Vec<u32>> = Vec::with_capacity(old.len() + visited.len() * cuts);
        let mut selv: HashSet<u32> = HashSet::new();
        for (fi, f) in old.iter().enumerate() {
            if let Some(&i) = visited.get(&(fi as u32)) {
                let (v0, v1, v2, v3) = (f[i], f[(i + 1) % 4], f[(i + 2) % 4], f[(i + 3) % 4]);
                let pa = orient(v0, v1);
                let pb = orient(v3, v2);
                let mut a_seq = vec![v0];
                a_seq.extend(&pa);
                a_seq.push(v1);
                let mut b_seq = vec![v3];
                b_seq.extend(&pb);
                b_seq.push(v2);
                for k in 0..=cuts {
                    out.push(vec![a_seq[k], a_seq[k + 1], b_seq[k + 1], b_seq[k]]);
                }
                selv.extend(pa);
                selv.extend(pb);
            } else {
                let n = f.len();
                let mut g = Vec::with_capacity(n + 2);
                for j in 0..n {
                    let (a, b) = (f[j], f[(j + 1) % n]);
                    g.push(a);
                    if newpts.contains_key(&key(a, b)) {
                        g.extend(orient(a, b));
                    }
                }
                out.push(g);
            }
        }
        self.faces = out;
        self.sel_v = selv;
        self.flush_from_verts();
        self.rebuild_edges();
        true
    }

    // ------------------------------------------------------------
    //  Subdivision (Catmull-Clark)
    // ------------------------------------------------------------

    pub fn subdivide(&mut self) -> bool {
        if self.faces.is_empty() || self.faces.len() > 40_000 {
            return false;
        }
        let (nv, ne, nf) = (self.verts.len(), self.edges.len(), self.faces.len());
        let emap = self.edge_map();

        let fp: Vec<Vec3> = (0..nf).map(|i| self.face_center(i)).collect();
        let mut ep: Vec<Vec3> = Vec::with_capacity(ne);
        for e in &self.edges {
            let (pa, pb) = (self.verts[e.a as usize], self.verts[e.b as usize]);
            ep.push(if e.f1 == NONE {
                (pa + pb) * 0.5
            } else {
                (pa + pb + fp[e.f0 as usize] + fp[e.f1 as usize]) * 0.25
            });
        }

        let mut fsum = vec![Vec3::ZERO; nv];
        let mut fcnt = vec![0u32; nv];
        for (fi, f) in self.faces.iter().enumerate() {
            for &v in f {
                fsum[v as usize] += fp[fi];
                fcnt[v as usize] += 1;
            }
        }
        let mut msum = vec![Vec3::ZERO; nv];
        let mut ecnt = vec![0u32; nv];
        let mut bsum = vec![Vec3::ZERO; nv];
        let mut bcnt = vec![0u32; nv];
        for e in &self.edges {
            let (a, b) = (e.a as usize, e.b as usize);
            let (pa, pb) = (self.verts[a], self.verts[b]);
            let mid = (pa + pb) * 0.5;
            msum[a] += mid;
            msum[b] += mid;
            ecnt[a] += 1;
            ecnt[b] += 1;
            if e.f1 == NONE {
                bsum[a] += pb;
                bsum[b] += pa;
                bcnt[a] += 1;
                bcnt[b] += 1;
            }
        }

        let mut newv: Vec<Vec3> = Vec::with_capacity(nv + ne + nf);
        for v in 0..nv {
            let p = self.verts[v];
            let np = if bcnt[v] >= 2 {
                if bcnt[v] == 2 { p * 0.75 + bsum[v] * 0.125 } else { p }
            } else if fcnt[v] == 0 || ecnt[v] < 3 {
                p
            } else {
                let n = ecnt[v] as f32;
                let f = fsum[v] / fcnt[v] as f32;
                let r = msum[v] / n;
                (f + r * 2.0 + p * (n - 3.0)) / n
            };
            newv.push(np);
        }
        newv.extend(ep);
        newv.extend(fp.iter().copied());

        let mut nfaces: Vec<Vec<u32>> = Vec::with_capacity(self.faces.iter().map(|f| f.len()).sum());
        for (fi, f) in self.faces.iter().enumerate() {
            let n = f.len();
            let fpi = (nv + ne + fi) as u32;
            for i in 0..n {
                let v = f[i];
                let nxt = f[(i + 1) % n];
                let prv = f[(i + n - 1) % n];
                let e1 = (nv + emap[&key(v, nxt)]) as u32;
                let e0 = (nv + emap[&key(prv, v)]) as u32;
                nfaces.push(vec![v, e1, fpi, e0]);
            }
        }
        self.verts = newv;
        self.faces = nfaces;
        self.select_none();
        self.rebuild_edges();
        true
    }

    // ------------------------------------------------------------
    //  Merge / Weld
    // ------------------------------------------------------------

    pub fn weld(&mut self, eps: f32) -> usize {
        let eps = eps.max(1e-6);
        let inv = 1.0 / eps;
        let before = self.verts.len();
        let mut grid: HashMap<(i32, i32, i32), u32> = HashMap::with_capacity(before);
        let mut remap = Vec::with_capacity(before);
        for (i, p) in self.verts.iter().enumerate() {
            let k = ((p.x * inv).round() as i32, (p.y * inv).round() as i32, (p.z * inv).round() as i32);
            let t = *grid.entry(k).or_insert(i as u32);
            remap.push(t);
        }
        self.apply_remap(&remap);
        before - self.verts.len()
    }

    pub fn merge_center(&mut self) -> bool {
        if self.sel_v.len() < 2 {
            return false;
        }
        let c = self.median();
        let first = *self.sel_v.iter().min().unwrap();
        self.verts[first as usize] = c;
        let mut remap: Vec<u32> = (0..self.verts.len() as u32).collect();
        for &v in &self.sel_v {
            remap[v as usize] = first;
        }
        self.apply_remap(&remap);
        true
    }

    // ------------------------------------------------------------
    //  Normals / Triangulate / Smooth / Mirror
    // ------------------------------------------------------------

    pub fn flip_normals(&mut self) -> bool {
        if self.faces.is_empty() {
            return false;
        }
        let all = self.sel_f.is_empty();
        for (i, f) in self.faces.iter_mut().enumerate() {
            if all || self.sel_f.contains(&(i as u32)) {
                f.reverse();
            }
        }
        self.rebuild_edges();
        true
    }

    pub fn recalc_outside(&mut self) {
        let nf = self.faces.len();
        if nf == 0 {
            return;
        }
        self.rebuild_edges();
        let mut adj: Vec<Vec<(u32, bool)>> = vec![Vec::new(); nf];
        let mut open_face = vec![false; nf];
        for e in &self.edges {
            if e.f1 == NONE {
                open_face[e.f0 as usize] = true;
                continue;
            }
            let same = has_dir(&self.faces[e.f0 as usize], e.a, e.b)
                == has_dir(&self.faces[e.f1 as usize], e.a, e.b);
            adj[e.f0 as usize].push((e.f1, same));
            adj[e.f1 as usize].push((e.f0, same));
        }
        let mut comp = vec![NONE; nf];
        let mut flip = vec![false; nf];
        let mut ncomp = 0u32;
        let mut stack: Vec<usize> = Vec::new();
        for s in 0..nf {
            if comp[s] != NONE {
                continue;
            }
            comp[s] = ncomp;
            stack.push(s);
            while let Some(f) = stack.pop() {
                for &(g, c) in &adj[f] {
                    let g = g as usize;
                    if comp[g] == NONE {
                        comp[g] = ncomp;
                        flip[g] = flip[f] ^ c;
                        stack.push(g);
                    }
                }
            }
            ncomp += 1;
        }
        let mut vol = vec![0.0f32; ncomp as usize];
        let mut open = vec![false; ncomp as usize];
        for fi in 0..nf {
            let c = comp[fi] as usize;
            if open_face[fi] {
                open[c] = true;
            }
            let f = &self.faces[fi];
            let a = self.verts[f[0] as usize];
            let mut v = 0.0;
            for k in 1..f.len() - 1 {
                let b = self.verts[f[k] as usize];
                let cc = self.verts[f[k + 1] as usize];
                v += a.dot(b.cross(cc)) / 6.0;
            }
            vol[c] += if flip[fi] { -v } else { v };
        }
        for fi in 0..nf {
            let c = comp[fi] as usize;
            let invert = !open[c] && vol[c] < 0.0;
            if flip[fi] ^ invert {
                self.faces[fi].reverse();
            }
        }
        self.rebuild_edges();
    }

    pub fn triangulate(&mut self) -> bool {
        let all = self.sel_f.is_empty();
        let old = std::mem::take(&mut self.faces);
        let mut changed = false;
        for (i, f) in old.into_iter().enumerate() {
            if f.len() > 3 && (all || self.sel_f.contains(&(i as u32))) {
                changed = true;
                for k in 1..f.len() - 1 {
                    self.faces.push(vec![f[0], f[k], f[k + 1]]);
                }
            } else {
                self.faces.push(f);
            }
        }
        self.sel_f.clear();
        self.flush_from_verts();
        self.rebuild_edges();
        changed
    }

    pub fn smooth(&mut self, iterations: u32, amount: f32) -> bool {
        let n = self.verts.len();
        if n == 0 {
            return false;
        }
        let mut boundary = vec![false; n];
        for e in &self.edges {
            if e.f1 == NONE {
                boundary[e.a as usize] = true;
                boundary[e.b as usize] = true;
            }
        }
        let all = self.sel_v.is_empty();
        let mut acc = vec![Vec3::ZERO; n];
        let mut cnt = vec![0u32; n];
        for _ in 0..iterations.max(1) {
            acc.iter_mut().for_each(|a| *a = Vec3::ZERO);
            cnt.iter_mut().for_each(|c| *c = 0);
            for e in &self.edges {
                let (a, b) = (e.a as usize, e.b as usize);
                acc[a] += self.verts[b];
                acc[b] += self.verts[a];
                cnt[a] += 1;
                cnt[b] += 1;
            }
            for v in 0..n {
                if cnt[v] == 0 || boundary[v] || !(all || self.sel_v.contains(&(v as u32))) {
                    continue;
                }
                let avg = acc[v] / cnt[v] as f32;
                self.verts[v] = self.verts[v].lerp(avg, amount);
            }
        }
        true
    }

    pub fn mirror(&mut self, axis: usize) -> bool {
        if self.faces.is_empty() || axis > 2 {
            return false;
        }
        let nv = self.verts.len();
        let mut map: Vec<u32> = Vec::with_capacity(nv);
        for i in 0..nv {
            let p = self.verts[i];
            if p[axis].abs() < 1e-4 {
                map.push(i as u32);
            } else {
                let mut q = p;
                q[axis] = -q[axis];
                self.verts.push(q);
                map.push(self.verts.len() as u32 - 1);
            }
        }
        let extra: Vec<Vec<u32>> = self
            .faces
            .iter()
            .map(|f| f.iter().rev().map(|&v| map[v as usize]).collect())
            .collect();
        self.faces.extend(extra);
        self.select_none();
        self.rebuild_edges();
        true
    }

    // ------------------------------------------------------------
    //  OBJ
    // ------------------------------------------------------------

    pub fn to_obj(&self) -> String {
        let mut s = String::with_capacity(self.verts.len() * 24 + self.faces.len() * 16);
        s.push_str("# LoadEngine\no Mesh\n");
        for v in &self.verts {
            s.push_str(&format!("v {} {} {}\n", v.x, v.y, v.z));
        }
        for f in &self.faces {
            s.push('f');
            for &i in f {
                s.push_str(&format!(" {}", i + 1));
            }
            s.push('\n');
        }
        s
    }

    pub fn from_obj(src: &str) -> EditMesh {
        let mut m = EditMesh::default();
        for line in src.lines() {
            let mut it = line.split_whitespace();
            match it.next() {
                Some("v") => {
                    let c: Vec<f32> = it.take(3).filter_map(|s| s.parse().ok()).collect();
                    if c.len() == 3 && m.verts.len() < 500_000 {
                        m.verts.push(Vec3::new(c[0], c[1], c[2]));
                    }
                }
                Some("f") => {
                    let nv = m.verts.len() as i64;
                    let idx: Vec<u32> = it
                        .filter_map(|t| {
                            let n: i64 = t.split('/').next()?.parse().ok()?;
                            let i = if n > 0 { n - 1 } else { nv + n };
                            if i >= 0 && i < nv { Some(i as u32) } else { None }
                        })
                        .collect();
                    if idx.len() >= 3 {
                        m.faces.push(idx);
                    }
                }
                _ => {}
            }
        }
        m.rebuild_edges();
        m
    }
}

// =====================================================================
//  Primitives
// =====================================================================

fn finish(mut m: EditMesh) -> EditMesh {
    m.recalc_outside();
    m
}

pub fn cube() -> EditMesh {
    let h = 1.0;
    let verts = vec![
        Vec3::new(-h, -h, -h),
        Vec3::new(h, -h, -h),
        Vec3::new(h, h, -h),
        Vec3::new(-h, h, -h),
        Vec3::new(-h, -h, h),
        Vec3::new(h, -h, h),
        Vec3::new(h, h, h),
        Vec3::new(-h, h, h),
    ];
    let faces = vec![
        vec![4, 5, 6, 7],
        vec![1, 0, 3, 2],
        vec![0, 4, 7, 3],
        vec![5, 1, 2, 6],
        vec![3, 7, 6, 2],
        vec![0, 1, 5, 4],
    ];
    finish(EditMesh { verts, faces, ..Default::default() })
}

pub fn grid(div: usize) -> EditMesh {
    let n = div.max(1);
    let mut verts = Vec::with_capacity((n + 1) * (n + 1));
    for i in 0..=n {
        for j in 0..=n {
            verts.push(Vec3::new(-1.0 + 2.0 * i as f32 / n as f32, 0.0, -1.0 + 2.0 * j as f32 / n as f32));
        }
    }
    let id = |i: usize, j: usize| (i * (n + 1) + j) as u32;
    let mut faces = Vec::with_capacity(n * n);
    for i in 0..n {
        for j in 0..n {
            faces.push(vec![id(i, j), id(i, j + 1), id(i + 1, j + 1), id(i + 1, j)]);
        }
    }
    let mut m = EditMesh { verts, faces, ..Default::default() };
    m.rebuild_edges();
    m
}

pub fn uv_sphere(seg: usize, rings: usize) -> EditMesh {
    let (seg, rings) = (seg.max(3), rings.max(2));
    let mut v = vec![Vec3::new(0.0, 1.0, 0.0)];
    for i in 1..rings {
        let phi = PI * i as f32 / rings as f32;
        let (y, r) = (phi.cos(), phi.sin());
        for j in 0..seg {
            let t = TAU * j as f32 / seg as f32;
            v.push(Vec3::new(r * t.cos(), y, r * t.sin()));
        }
    }
    v.push(Vec3::new(0.0, -1.0, 0.0));
    let bottom = (v.len() - 1) as u32;
    let ring = |i: usize, j: usize| (1 + (i - 1) * seg + (j % seg)) as u32;
    let mut f = Vec::new();
    for j in 0..seg {
        f.push(vec![0, ring(1, j + 1), ring(1, j)]);
    }
    for i in 1..rings - 1 {
        for j in 0..seg {
            f.push(vec![ring(i, j), ring(i, j + 1), ring(i + 1, j + 1), ring(i + 1, j)]);
        }
    }
    for j in 0..seg {
        f.push(vec![bottom, ring(rings - 1, j), ring(rings - 1, j + 1)]);
    }
    finish(EditMesh { verts: v, faces: f, ..Default::default() })
}

pub fn cylinder(seg: usize) -> EditMesh {
    let n = seg.max(3);
    let mut v = Vec::with_capacity(2 * n);
    for y in [-1.0f32, 1.0] {
        for i in 0..n {
            let t = TAU * i as f32 / n as f32;
            v.push(Vec3::new(t.cos(), y, t.sin()));
        }
    }
    let mut f = Vec::new();
    for i in 0..n {
        let j = (i + 1) % n;
        f.push(vec![i as u32, j as u32, (n + j) as u32, (n + i) as u32]);
    }
    f.push((0..n as u32).collect());
    f.push((n as u32..2 * n as u32).collect());
    finish(EditMesh { verts: v, faces: f, ..Default::default() })
}

pub fn cone(seg: usize) -> EditMesh {
    let n = seg.max(3);
    let mut v = Vec::with_capacity(n + 1);
    for i in 0..n {
        let t = TAU * i as f32 / n as f32;
        v.push(Vec3::new(t.cos(), -1.0, t.sin()));
    }
    v.push(Vec3::new(0.0, 1.0, 0.0));
    let apex = n as u32;
    let mut f = Vec::new();
    for i in 0..n {
        f.push(vec![i as u32, ((i + 1) % n) as u32, apex]);
    }
    f.push((0..n as u32).collect());
    finish(EditMesh { verts: v, faces: f, ..Default::default() })
}

pub fn torus(major: usize, minor: usize) -> EditMesh {
    let (a, b) = (major.max(3), minor.max(3));
    let (big, small) = (1.0f32, 0.35f32);
    let mut v = Vec::with_capacity(a * b);
    for i in 0..a {
        let u = TAU * i as f32 / a as f32;
        for j in 0..b {
            let w = TAU * j as f32 / b as f32;
            let r = big + small * w.cos();
            v.push(Vec3::new(r * u.cos(), small * w.sin(), r * u.sin()));
        }
    }
    let id = |i: usize, j: usize| ((i % a) * b + (j % b)) as u32;
    let mut f = Vec::new();
    for i in 0..a {
        for j in 0..b {
            f.push(vec![id(i, j), id(i + 1, j), id(i + 1, j + 1), id(i, j + 1)]);
        }
    }
    finish(EditMesh { verts: v, faces: f, ..Default::default() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn euler(m: &EditMesh) -> i64 {
        m.verts.len() as i64 - m.edges.len() as i64 + m.faces.len() as i64
    }

    #[test]
    fn cube_topology() {
        let m = cube();
        assert_eq!((m.verts.len(), m.edges.len(), m.faces.len()), (8, 12, 6));
        assert!(m.edges.iter().all(|e| e.f1 != NONE));
    }

    #[test]
    fn subdivide_keeps_euler() {
        let mut m = cube();
        assert!(m.subdivide());
        assert_eq!((m.verts.len(), m.faces.len()), (26, 24));
        assert_eq!(euler(&m), 2);
    }

    #[test]
    fn loop_cut_closed_ring() {
        let mut m = cube();
        m.sel_v.insert(m.edges[0].a);
        m.sel_v.insert(m.edges[0].b);
        assert!(m.loop_cut(1));
        assert_eq!(m.faces.len(), 10);
        assert_eq!(euler(&m), 2);
    }

    #[test]
    fn extrude_plane_face() {
        let mut m = grid(1);
        m.sel_f.insert(0);
        assert!(m.extrude_faces());
        assert_eq!(m.faces.len(), 5);
    }
}