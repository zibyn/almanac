//! Lazy source check, `verify` and stale flags (spec ch. 3 「惰性核验」「verify」, ch. 6).
mod common;
use common::Almanac;
use serde_json::{Value, json};
use std::path::Path;

/// A non-git project dir holding `a.txt`.
fn project(content: &str) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("a.txt"), content).unwrap();
    d
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// Records a global memory citing one file evidence of `a.txt`; returns (ev, mem).
fn remember_file(a: &Almanac, d: &Path, excerpt: &str) -> (String, String) {
    let r = a
        .tool(
            "record",
            json!({
                "project": s(d),
                "evidence": [{"ref": "e", "excerpt": excerpt, "source": {"type": "file", "path": "a.txt"}}],
                "memories": [{"text": format!("记忆 {excerpt}"), "scope": "global", "evidence": ["e"]}]
            }),
        )
        .ok();
    (
        r["evidence"][0]["id"].as_str().unwrap().into(),
        r["memories"][0]["id"].as_str().unwrap().into(),
    )
}

fn get(a: &Almanac, d: &Path, id: &str) -> Value {
    a.tool("get", json!({"ids": [id], "project": s(d)})).ok()["records"][0].clone()
}

#[test]
fn get_flags_memory_whose_file_excerpt_is_gone() {
    let d = project("构建用 cargo build\n");
    let a = Almanac::new();
    let (ev, mem) = remember_file(&a, d.path(), "cargo build");
    let m = get(&a, d.path(), &mem);
    assert_eq!(m["stale"], Value::Null);
    assert_eq!(m["evidence"][0]["check"]["result"], "match");

    std::fs::write(d.path().join("a.txt"), "构建用 make\n").unwrap();
    let m = get(&a, d.path(), &mem);
    assert_eq!(m["stale"], json!([{"evidence": ev, "result": "changed"}]));
    assert_eq!(m["evidence"][0]["check"]["result"], "changed");
    // The result was written to the cache: the evidence record alone now shows it.
    assert_eq!(get(&a, d.path(), &ev)["check"]["result"], "changed");
}

#[test]
fn search_flags_hits_and_stale_only_filters_all_candidates() {
    let d = project("cargo build\nmake all\n");
    let a = Almanac::new();
    let (ev, broken) = remember_file(&a, d.path(), "cargo build");
    let (_, fine) = remember_file(&a, d.path(), "make all");
    std::fs::write(d.path().join("a.txt"), "make all\n").unwrap();

    let r = a.tool("search", json!({"project": s(d.path())})).ok();
    let hits = r["hits"].as_array().unwrap();
    assert_eq!(hits[0]["id"], fine.as_str());
    assert_eq!(hits[0]["stale"], Value::Null);
    assert_eq!(
        hits[1]["stale"],
        json!([{"evidence": ev, "result": "changed"}])
    );

    // Fresh cache: the broken memory is not among the newest `limit` hits, yet stale_only finds it.
    std::fs::remove_dir_all(a.home.path().join("cache")).unwrap();
    let r = a
        .tool(
            "search",
            json!({"project": s(d.path()), "stale_only": true, "limit": 1}),
        )
        .ok();
    let hits = r["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["id"], broken.as_str());
}

fn check_of(m: &Value) -> &Value {
    &m["evidence"][0]["check"]["result"]
}

#[test]
fn excerpt_still_present_matches_whatever_else_changed() {
    let d = project("头\n构建用 cargo build\n尾\n");
    let a = Almanac::new();
    let (_, mem) = remember_file(&a, d.path(), "构建用 cargo   build");
    std::fs::write(
        d.path().join("a.txt"),
        "全新的开头\n\n构建用 cargo\nbuild\n",
    )
    .unwrap();
    let m = get(&a, d.path(), &mem);
    assert_eq!(check_of(&m), "match");
    assert_eq!(m["stale"], Value::Null);
}

#[test]
fn missing_file_is_unreachable() {
    let d = project("cargo build");
    let a = Almanac::new();
    let (ev, mem) = remember_file(&a, d.path(), "cargo build");
    std::fs::remove_file(d.path().join("a.txt")).unwrap();
    let m = get(&a, d.path(), &mem);
    assert_eq!(
        m["stale"],
        json!([{"evidence": ev, "result": "unreachable"}])
    );
}

#[test]
fn invalid_utf8_is_decoded_lossily() {
    let d = project("x");
    std::fs::write(d.path().join("a.txt"), b"\xff\xfe cargo build \xc3").unwrap();
    let a = Almanac::new();
    let (_, mem) = remember_file(&a, d.path(), "cargo build");
    std::fs::write(d.path().join("a.txt"), b"\xff cargo build!").unwrap();
    assert_eq!(check_of(&get(&a, d.path(), &mem)), "match");
}

#[test]
fn unchanged_mtime_and_size_reuse_the_cache_without_reading() {
    let d = project("cargo build");
    let path = d.path().join("a.txt");
    let mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
    let a = Almanac::new();
    let (_, mem) = remember_file(&a, d.path(), "cargo build");
    // Same size, mtime put back: had the file been read, the result would be `changed`.
    std::fs::write(&path, "make  build").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
    assert_eq!(check_of(&get(&a, d.path(), &mem)), "match");
    // A different size is read again.
    std::fs::write(&path, "make build").unwrap();
    assert_eq!(check_of(&get(&a, d.path(), &mem)), "changed");
}

#[test]
fn file_of_a_project_not_located_here_is_not_checked() {
    let d = project("cargo build");
    let elsewhere = tempfile::tempdir().unwrap();
    let a = Almanac::new();
    let (_, mem) = remember_file(&a, d.path(), "cargo build");
    std::fs::write(d.path().join("a.txt"), "make").unwrap();
    // The cached write-time result stays.
    let m = get(&a, elsewhere.path(), &mem);
    assert_eq!(check_of(&m), "match");
    assert_eq!(m["stale"], Value::Null);
    // Without any cached result: unchecked, and no flag.
    std::fs::remove_dir_all(a.home.path().join("cache")).unwrap();
    let m = get(&a, elsewhere.path(), &mem);
    assert_eq!(m["evidence"][0]["check"], Value::Null);
    assert_eq!(m["stale"], Value::Null);
}

#[test]
fn deleting_the_cache_recovers_on_the_next_check() {
    let d = project("cargo build");
    let a = Almanac::new();
    let (ev, mem) = remember_file(&a, d.path(), "cargo build");
    std::fs::write(d.path().join("a.txt"), "make").unwrap();
    std::fs::remove_dir_all(a.home.path().join("cache")).unwrap();
    let m = get(&a, d.path(), &mem);
    assert_eq!(m["stale"], json!([{"evidence": ev, "result": "changed"}]));
}

#[test]
fn only_the_latest_evidence_bearing_event_counts() {
    let d = project("cargo build\nmake all\n");
    let a = Almanac::new();
    let (_, mem) = remember_file(&a, d.path(), "cargo build");
    a.tool(
        "corroborate",
        json!({"memory": mem, "project": s(d.path()),
               "evidence": [{"excerpt": "make all", "source": {"type": "file", "path": "a.txt"}}]}),
    )
    .ok();
    // The older evidence breaks: the corroboration already judged it, so no flag.
    std::fs::write(d.path().join("a.txt"), "make all\n").unwrap();
    let m = get(&a, d.path(), &mem);
    assert_eq!(check_of(&m), "changed");
    assert_eq!(m["stale"], Value::Null);
    // The corroboration's own evidence breaking does flag it.
    std::fs::write(d.path().join("a.txt"), "nothing\n").unwrap();
    let m = get(&a, d.path(), &mem);
    assert_eq!(m["stale"][0]["evidence"], m["evidence"][1]["id"]);
}

#[test]
fn verify_checks_each_cited_evidence_once_and_reports_stale() {
    let d = project("cargo build\n");
    let a = Almanac::new();
    let r = a
        .tool(
            "record",
            json!({
                "project": s(d.path()),
                "evidence": [
                    {"ref": "f", "excerpt": "cargo build", "source": {"type": "file", "path": "a.txt"}},
                    {"ref": "c", "excerpt": "用户说用 cargo", "source": {"type": "conversation"}},
                    {"ref": "x", "excerpt": "ok", "source": {"type": "command", "command": "make", "exit_code": 0}}
                ],
                "memories": [
                    {"text": "构建用 cargo", "scope": "global", "evidence": ["f", "c"]},
                    {"text": "make 能跑", "scope": "global", "evidence": ["f", "x"]}
                ]
            }),
        )
        .ok();
    let ev = |i: usize| r["evidence"][i]["id"].as_str().unwrap().to_string();
    let mem = |i: usize| r["memories"][i]["id"].as_str().unwrap().to_string();
    std::fs::write(d.path().join("a.txt"), "make\n").unwrap();

    let v = a
        .tool(
            "verify",
            json!({"ids": [mem(0), mem(1)], "project": s(d.path())}),
        )
        .ok();
    let checked: Vec<(&str, &str, &str)> = v["checked"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            assert!(c["at"].as_str().unwrap().ends_with('Z'), "{c}");
            (
                c["evidence"].as_str().unwrap(),
                c["source_type"].as_str().unwrap(),
                c["result"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        checked,
        [
            (ev(0).as_str(), "file", "changed"),
            (ev(1).as_str(), "conversation", "unverifiable"),
            (ev(2).as_str(), "command", "unverifiable"),
        ]
    );
    assert_eq!(v["network_requests"], json!([]));
    let flag = json!([{"evidence": ev(0), "result": "changed"}]);
    assert_eq!(
        v["stale"],
        json!([
            {"memory": mem(0), "text": "构建用 cargo", "stale": flag},
            {"memory": mem(1), "text": "make 能跑", "stale": flag}
        ])
    );
    // Results land in the cache that `get` reads.
    assert_eq!(get(&a, d.path(), &ev(1))["check"]["result"], "unverifiable");
}

/// A local HTTP server answering `route(path)` = (status line, content type, body); counts requests.
fn serve(
    route: fn(&str) -> (&'static str, &'static str, String),
) -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    use std::io::{BufRead, BufReader, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = hits.clone();
    std::thread::spawn(move || {
        for conn in l.incoming() {
            let mut conn = conn.unwrap();
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::thread::spawn(move || {
                let mut r = BufReader::new(conn.try_clone().unwrap());
                let mut line = String::new();
                r.read_line(&mut line).unwrap();
                let path = line.split(' ').nth(1).unwrap_or("/").to_string();
                loop {
                    let mut h = String::new();
                    if r.read_line(&mut h).unwrap() <= 2 {
                        break;
                    }
                }
                let (status, ty, body) = route(&path);
                let _ = write!(
                    conn,
                    "HTTP/1.1 {status}\r\nContent-Type: {ty}\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n{body}",
                    body.len(),
                    path.strip_prefix("/redir/")
                        .and_then(|n| n.parse::<u32>().ok())
                        .filter(|n| *n > 0)
                        .map(|n| format!("Location: /redir/{}\r\n", n - 1))
                        .unwrap_or_default()
                );
            });
        }
    });
    (base, hits)
}

fn route(path: &str) -> (&'static str, &'static str, String) {
    let page = "<html><head><style>.x{}</style><script>var s = \"脚本里的字\";</script></head>\
                <body><p>构建用 <b>cargo</b>&nbsp;build &amp;&#32;test</p></body></html>";
    match path {
        "/html" => ("200 OK", "text/html; charset=utf-8", page.into()),
        "/plain" => ("200 OK", "text/plain", "别的内容".into()),
        "/404" => ("404 Not Found", "text/plain", "构建用 cargo build".into()),
        "/png" => ("200 OK", "image/png", "构建用 cargo build".into()),
        "/slow" => {
            std::thread::sleep(std::time::Duration::from_secs(12));
            ("200 OK", "text/plain", "构建用 cargo build".into())
        }
        p if p.starts_with("/redir/") => match p {
            "/redir/0" => ("200 OK", "text/plain", "构建用 cargo build".into()),
            _ => ("302 Found", "text/plain", String::new()),
        },
        _ => ("404 Not Found", "text/plain", String::new()),
    }
}

/// Records one global memory per (url, excerpt); returns the memory ids.
fn remember_web(a: &Almanac, pages: &[(String, &str)]) -> Vec<String> {
    pages
        .iter()
        .enumerate()
        .map(|(i, (url, excerpt))| {
            let r = a
                .tool(
                    "record",
                    json!({
                        "evidence": [{"ref": "e", "excerpt": excerpt, "source": {"type": "web", "url": url}}],
                        "memories": [{"text": format!("网页 {i}"), "scope": "global", "evidence": ["e"]}]
                    }),
                )
                .ok();
            r["memories"][0]["id"].as_str().unwrap().to_string()
        })
        .collect()
}

#[test]
fn verify_fetches_web_evidence_and_judges_it() {
    let (base, requests) = serve(route);
    let u = |p: &str| format!("{base}{p}");
    let cases = [
        (u("/html"), "构建用 cargo build & test", "match"),
        (u("/html"), "脚本里的字", "changed"),
        (u("/plain"), "构建用 cargo build", "changed"),
        (u("/404"), "构建用 cargo build", "unreachable"),
        (u("/png"), "构建用 cargo build", "changed"),
        (u("/redir/5"), "构建用 cargo build", "match"),
        (u("/redir/6"), "构建用 cargo build", "unreachable"),
        (u("/slow"), "构建用 cargo build", "unreachable"),
    ];
    let a = Almanac::new();
    let mems = remember_web(
        &a,
        &cases
            .iter()
            .map(|(u, e, _)| (u.clone(), *e))
            .collect::<Vec<_>>(),
    );

    // search and get never touch the network; verify with include_web: false skips web evidence.
    a.tool("search", json!({})).ok();
    a.tool("get", json!({"ids": mems})).ok();
    let v = a
        .tool("verify", json!({"ids": mems, "include_web": false}))
        .ok();
    assert_eq!(v["checked"], json!([]));
    assert_eq!(v["network_requests"], json!([]));
    assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 0);

    let v = a.tool("verify", json!({"ids": mems})).ok();
    let got: Vec<&str> = v["checked"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["result"].as_str().unwrap())
        .collect();
    let want: Vec<&str> = cases.iter().map(|c| c.2).collect();
    assert_eq!(got, want);
    let urls: Vec<String> = cases.iter().map(|c| c.0.clone()).collect();
    assert_eq!(v["network_requests"], json!(urls));
    assert_eq!(v["stale"].as_array().unwrap().len(), 6);
    assert_eq!(v["checked"][0]["source_type"], "web");
}

#[test]
fn verify_unknown_memory_is_memory_not_found() {
    let a = Almanac::new();
    a.tool("verify", json!({"ids": ["mem_nope"]}))
        .err("MEMORY_NOT_FOUND");
}

#[test]
fn verify_without_ids_covers_active_memories_in_scope() {
    let d = project("cargo build\n");
    let a = Almanac::new();
    let (ev, _) = remember_file(&a, d.path(), "cargo build");
    let (_, old) = a.remember("撤回的");
    a.tool("retract", json!({"memory": old, "reason": "错了"}))
        .ok();
    let v = a
        .tool("verify", json!({"scope": "global", "project": s(d.path())}))
        .ok();
    let checked: Vec<&str> = v["checked"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["evidence"].as_str().unwrap())
        .collect();
    assert_eq!(checked, [ev.as_str()]);
    assert_eq!(v["stale"], json!([]));
    // Project scope: no project memories here.
    let v = a
        .tool(
            "verify",
            json!({"scope": "project", "project": s(d.path())}),
        )
        .ok();
    assert_eq!(v["checked"], json!([]));
}

#[test]
fn purged_evidence_does_not_make_a_memory_stale() {
    let d = project("构建用 cargo build\n");
    let a = Almanac::new();
    let (ev, mem) = remember_file(&a, d.path(), "cargo build");
    std::fs::write(d.path().join("a.txt"), "构建用 make\n").unwrap();
    assert_eq!(
        get(&a, d.path(), &mem)["stale"],
        json!([{"evidence": ev, "result": "changed"}])
    );

    // A cache entry written for the evidence before it was purged (purge deletes the cache;
    // a concurrent reader that loaded the log earlier may write it back).
    let cache = a.home.path().join("cache/checks.json");
    let saved = std::fs::read(&cache).unwrap();
    let o = a.run_tty(&["purge", &ev, "--reason", "测试"], "n\nyes\n");
    assert_eq!(o.code, 0, "{}", o.stdout);
    std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
    std::fs::write(&cache, saved).unwrap();

    assert_eq!(get(&a, d.path(), &mem)["stale"], Value::Null);
}
