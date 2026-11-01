//! The state tree: one versioned JSON document of live app
//! state, changed only through `commit`, which turns each change into RFC 6902
//! patch ops, keeps the last few thousand for `watch`, and tells subscribers.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::registry::Actor;

/// Patches kept for `watch` replays; older `since` values get a snapshot.
pub const WINDOW: usize = 2000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Op {
    Add { path: String, value: Value },
    Replace { path: String, value: Value },
    Remove { path: String },
}

impl Op {
    pub fn path(&self) -> &str {
        match self {
            Op::Add { path, .. } | Op::Replace { path, .. } | Op::Remove { path } => path,
        }
    }
}

/// Why the tree changed: a command (with its event-log id once  lands) or the runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Cause {
    Command {
        #[serde(rename = "eventId", default, skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
        command: String,
        actor: Actor,
    },
    Runtime {
        runtime: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Patch {
    pub epoch: String,
    /// The version after applying `ops`.
    pub version: u64,
    pub ops: Vec<Op>,
    pub cause: Cause,
}

type Subscriber = Box<dyn Fn(&Patch) + Send + Sync>;

struct Inner {
    version: u64,
    doc: Value,
    recent: VecDeque<Patch>,
}

/// Cheap to clone: clones share one tree.
#[derive(Clone)]
pub struct StateTree {
    epoch: Arc<str>,
    inner: Arc<(Mutex<Inner>, Condvar)>,
    subscribers: Arc<Mutex<Vec<Subscriber>>>,
}

impl StateTree {
    pub fn new(initial: Value) -> Self {
        StateTree {
            epoch: ulid::Ulid::generate().to_string().into(),
            inner: Arc::new((
                Mutex::new(Inner {
                    version: 0,
                    doc: initial,
                    recent: VecDeque::new(),
                }),
                Condvar::new(),
            )),
            subscribers: Arc::default(),
        }
    }

    pub fn epoch(&self) -> &str {
        &self.epoch
    }

    pub fn version(&self) -> u64 {
        self.lock().version
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The subtree at `pointer` (RFC 6901; "" is the whole document), with the version read.
    pub fn get(&self, pointer: &str) -> Option<(u64, Value)> {
        let g = self.lock();
        g.doc.pointer(pointer).map(|v| (g.version, v.clone()))
    }

    /// Calls `f` with every patch from now on (the UI's `state:patch`, SSE, MCP resources).
    pub fn subscribe(&self, f: impl Fn(&Patch) + Send + Sync + 'static) {
        self.subscribers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(Box::new(f));
    }

    /// Changes the document. A change that alters nothing bumps no version and
    /// sends nothing. Returns the patch, if there was one.
    pub fn commit(&self, cause: Cause, f: impl FnOnce(&mut Value)) -> Option<Patch> {
        let patch = {
            let mut g = self.lock();
            let mut next = g.doc.clone();
            f(&mut next);
            let mut ops = Vec::new();
            diff("", &g.doc, &next, &mut ops);
            if ops.is_empty() {
                return None;
            }
            g.doc = next;
            g.version += 1;
            let patch = Patch {
                epoch: self.epoch.to_string(),
                version: g.version,
                ops,
                cause,
            };
            g.recent.push_back(patch.clone());
            if g.recent.len() > WINDOW {
                g.recent.pop_front();
            }
            patch
        };
        self.inner.1.notify_all();
        for s in self
            .subscribers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
        {
            s(&patch);
        }
        Some(patch)
    }
}

/// RFC 6901 escaping for one key.
pub fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// Minimal ops turning `a` into `b`. Objects are diffed per key; arrays and
/// scalars are replaced whole (entities live in maps keyed by id).
pub fn diff(path: &str, a: &Value, b: &Value, ops: &mut Vec<Op>) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, va) in x {
                let p = format!("{path}/{}", escape(k));
                match y.get(k) {
                    Some(vb) => diff(&p, va, vb, ops),
                    None => ops.push(Op::Remove { path: p }),
                }
            }
            for (k, vb) in y {
                if !x.contains_key(k) {
                    ops.push(Op::Add {
                        path: format!("{path}/{}", escape(k)),
                        value: vb.clone(),
                    });
                }
            }
        }
        _ if a != b => ops.push(Op::Replace {
            path: path.into(),
            value: b.clone(),
        }),
        _ => {}
    }
}

/// Applies ops (the same algorithm the UI store uses). Errors name the bad path.
pub fn apply(doc: &mut Value, ops: &[Op]) -> Result<(), String> {
    for op in ops {
        let path = op.path();
        if path.is_empty() {
            match op {
                Op::Add { value, .. } | Op::Replace { value, .. } => *doc = value.clone(),
                Op::Remove { .. } => *doc = Value::Null,
            }
            continue;
        }
        let (parent, key) = path
            .rsplit_once('/')
            .ok_or_else(|| format!("bad path {path}"))?;
        let key = key.replace("~1", "/").replace("~0", "~");
        let obj = doc
            .pointer_mut(parent)
            .and_then(Value::as_object_mut)
            .ok_or_else(|| format!("no object at {parent}"))?;
        match op {
            Op::Add { value, .. } | Op::Replace { value, .. } => {
                obj.insert(key, value.clone());
            }
            Op::Remove { .. } => {
                obj.remove(&key)
                    .ok_or_else(|| format!("nothing at {path}"))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rt() -> Cause {
        Cause::Runtime {
            runtime: "test".into(),
        }
    }

    fn tree() -> StateTree {
        StateTree::new(json!({ "threads": {}, "settings": { "tint": "blue" } }))
    }

    #[test]
    fn commit_makes_minimal_ops_and_bumps_version() {
        let t = tree();
        let p = t
            .commit(rt(), |d| {
                d["threads"]["01A"] = json!({ "id": "01A", "status": "running" });
                d["settings"]["tint"] = json!("green");
            })
            .unwrap();
        assert_eq!(p.version, 1);
        assert_eq!(
            p.ops,
            vec![
                Op::Replace {
                    path: "/settings/tint".into(),
                    value: json!("green")
                },
                Op::Add {
                    path: "/threads/01A".into(),
                    value: json!({ "id": "01A", "status": "running" })
                },
            ]
        );
        let p = t
            .commit(rt(), |d| d["threads"]["01A"]["status"] = json!("waiting"))
            .unwrap();
        assert_eq!(
            p.ops,
            vec![Op::Replace {
                path: "/threads/01A/status".into(),
                value: json!("waiting")
            }]
        );
        assert!(t
            .commit(rt(), |d| d["threads"]["01A"]["status"] = json!("waiting"))
            .is_none());
        assert_eq!(t.version(), 2);
        assert_eq!(t.get("/threads/01A/status").unwrap(), (2, json!("waiting")));
    }

    #[test]
    fn patches_round_trip() {
        let a = json!({ "x": { "a/b": 1, "t~": [1, 2], "gone": true }, "y": null });
        let b = json!({ "x": { "a/b": 2, "t~": [3], "new": { "k": "v" } }, "y": 5 });
        let mut ops = Vec::new();
        diff("", &a, &b, &mut ops);
        let mut c = a.clone();
        apply(&mut c, &ops).unwrap();
        assert_eq!(c, b);
        assert!(ops.iter().any(|o| o.path() == "/x/a~1b"));
    }

    #[test]
    fn op_json_is_rfc6902() {
        let v = serde_json::to_value(Op::Remove { path: "/a".into() }).unwrap();
        assert_eq!(v, json!({ "op": "remove", "path": "/a" }));
    }

    #[test]
    fn subscribers_hear_every_patch() {
        let t = tree();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        t.subscribe(move |p| s.lock().unwrap().push(p.version));
        t.commit(rt(), |d| d["settings"]["tint"] = json!("amber"));
        t.commit(rt(), |d| d["settings"]["tint"] = json!("amber"));
        t.commit(rt(), |d| d["settings"]["tint"] = json!("red"));
        assert_eq!(*seen.lock().unwrap(), vec![1, 2]);
    }
}
