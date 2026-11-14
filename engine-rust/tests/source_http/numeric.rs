use super::*;
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_create_cores_1_5_c1dc18,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[create-cores-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["cores"]=json!(1.5);let r=h.post("/api/profiles",bad,None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("cores"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_create_mediavideo_1_5_dc2916,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[create-mediaVideo-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["mediaVideo"]=json!(1.5);let r=h.post("/api/profiles",bad,None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("mediaVideo"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_create_mediaaudio_1_5_4ae620,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[create-mediaAudio-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["mediaAudio"]=json!(1.5);let r=h.post("/api/profiles",bad,None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("mediaAudio"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_create_mediaspeakers_1_5_2c9707,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[create-mediaSpeakers-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["mediaSpeakers"]=json!(1.5);let r=h.post("/api/profiles",bad,None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("mediaSpeakers"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_create_maxtouchpoints_1_5_91b1bc,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[create-maxTouchPoints-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["maxTouchPoints"]=json!(1.5);let r=h.post("/api/profiles",bad,None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("maxTouchPoints"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_create_seed_1_5_ce3e5c,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[create-seed-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["seed"]=json!(1.5);let r=h.post("/api/profiles",bad,None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("seed"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_create_memory_3_5_c9d219,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[create-memory-3.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["memory"]=json!(3.5);let r=h.post("/api/profiles",bad,None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("memory"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_update_cores_1_5_f1f732,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[update-cores-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["cores"]=json!(1.5);let r=h.put(&format!("/api/profiles/{}",text(&a,"id")),json!({"cores":1.5}));assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("cores"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_update_mediavideo_1_5_79742b,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[update-mediaVideo-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["mediaVideo"]=json!(1.5);let r=h.put(&format!("/api/profiles/{}",text(&a,"id")),json!({"mediaVideo":1.5}));assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("mediaVideo"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_update_mediaaudio_1_5_8867a1,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[update-mediaAudio-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["mediaAudio"]=json!(1.5);let r=h.put(&format!("/api/profiles/{}",text(&a,"id")),json!({"mediaAudio":1.5}));assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("mediaAudio"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_update_mediaspeakers_1_5_b3c314,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[update-mediaSpeakers-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["mediaSpeakers"]=json!(1.5);let r=h.put(&format!("/api/profiles/{}",text(&a,"id")),json!({"mediaSpeakers":1.5}));assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("mediaSpeakers"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_update_maxtouchpoints_1_5_26735d,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[update-maxTouchPoints-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["maxTouchPoints"]=json!(1.5);let r=h.put(&format!("/api/profiles/{}",text(&a,"id")),json!({"maxTouchPoints":1.5}));assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("maxTouchPoints"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_update_seed_1_5_9af968,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[update-seed-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["seed"]=json!(1.5);let r=h.put(&format!("/api/profiles/{}",text(&a,"id")),json!({"seed":1.5}));assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("seed"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_update_memory_3_5_512b0b,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[update-memory-3.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["memory"]=json!(3.5);let r=h.put(&format!("/api/profiles/{}",text(&a,"id")),json!({"memory":3.5}));assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("memory"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_batch_cores_1_5_0b4926,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[batch-cores-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["cores"]=json!(1.5);let r=h.post("/api/profiles/batch",json!({"profiles":[{"name":"valid-new","engine":"playwright"},bad]}),None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("cores"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_batch_mediavideo_1_5_f793e5,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[batch-mediaVideo-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["mediaVideo"]=json!(1.5);let r=h.post("/api/profiles/batch",json!({"profiles":[{"name":"valid-new","engine":"playwright"},bad]}),None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("mediaVideo"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_batch_mediaaudio_1_5_b10b39,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[batch-mediaAudio-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["mediaAudio"]=json!(1.5);let r=h.post("/api/profiles/batch",json!({"profiles":[{"name":"valid-new","engine":"playwright"},bad]}),None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("mediaAudio"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_batch_mediaspeakers_1_5_9f109a,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[batch-mediaSpeakers-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["mediaSpeakers"]=json!(1.5);let r=h.post("/api/profiles/batch",json!({"profiles":[{"name":"valid-new","engine":"playwright"},bad]}),None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("mediaSpeakers"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_batch_maxtouchpoints_1_5_43448d,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[batch-maxTouchPoints-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["maxTouchPoints"]=json!(1.5);let r=h.post("/api/profiles/batch",json!({"profiles":[{"name":"valid-new","engine":"playwright"},bad]}),None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("maxTouchPoints"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_batch_seed_1_5_f005cf,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[batch-seed-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["seed"]=json!(1.5);let r=h.post("/api/profiles/batch",json!({"profiles":[{"name":"valid-new","engine":"playwright"},bad]}),None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("seed"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_batch_memory_3_5_624acc,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[batch-memory-3.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["memory"]=json!(3.5);let r=h.post("/api/profiles/batch",json!({"profiles":[{"name":"valid-new","engine":"playwright"},bad]}),None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("memory"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_bulk_cores_1_5_561012,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[bulk-cores-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["cores"]=json!(1.5);let r=h.post("/api/profiles/bulk",json!({"ids":[a["id"],b["id"]],"patch":{"cores":1.5}}),None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("cores"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_bulk_mediavideo_1_5_1877a9,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[bulk-mediaVideo-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["mediaVideo"]=json!(1.5);let r=h.post("/api/profiles/bulk",json!({"ids":[a["id"],b["id"]],"patch":{"mediaVideo":1.5}}),None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("mediaVideo"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_bulk_mediaaudio_1_5_1a2ef6,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[bulk-mediaAudio-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["mediaAudio"]=json!(1.5);let r=h.post("/api/profiles/bulk",json!({"ids":[a["id"],b["id"]],"patch":{"mediaAudio":1.5}}),None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("mediaAudio"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_bulk_mediaspeakers_1_5_cbb237,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[bulk-mediaSpeakers-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["mediaSpeakers"]=json!(1.5);let r=h.post("/api/profiles/bulk",json!({"ids":[a["id"],b["id"]],"patch":{"mediaSpeakers":1.5}}),None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("mediaSpeakers"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_bulk_maxtouchpoints_1_5_9db846,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[bulk-maxTouchPoints-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["maxTouchPoints"]=json!(1.5);let r=h.post("/api/profiles/bulk",json!({"ids":[a["id"],b["id"]],"patch":{"maxTouchPoints":1.5}}),None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("maxTouchPoints"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_bulk_seed_1_5_046fd3,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[bulk-seed-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["seed"]=json!(1.5);let r=h.post("/api/profiles/bulk",json!({"ids":[a["id"],b["id"]],"patch":{"seed":1.5}}),None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("seed"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_bulk_memory_3_5_c04e37,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[bulk-memory-3.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["memory"]=json!(3.5);let r=h.post("/api/profiles/bulk",json!({"ids":[a["id"],b["id"]],"patch":{"memory":3.5}}),None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("memory"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_native_import_cores_1_5_44f363,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[native-import-cores-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["cores"]=json!(1.5);let r=h.post("/api/profiles/import",json!({"text":json!({"numeraProfiles":1,"profiles":[{"name":"valid-import","engine":"playwright"},bad]}).to_string()}),None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("cores"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_native_import_mediavideo_1_5_a8b211,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[native-import-mediaVideo-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["mediaVideo"]=json!(1.5);let r=h.post("/api/profiles/import",json!({"text":json!({"numeraProfiles":1,"profiles":[{"name":"valid-import","engine":"playwright"},bad]}).to_string()}),None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("mediaVideo"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_native_import_mediaaudio_1_5_225e29,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[native-import-mediaAudio-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["mediaAudio"]=json!(1.5);let r=h.post("/api/profiles/import",json!({"text":json!({"numeraProfiles":1,"profiles":[{"name":"valid-import","engine":"playwright"},bad]}).to_string()}),None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("mediaAudio"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_native_import_mediaspeakers_1_5_440c1d,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[native-import-mediaSpeakers-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["mediaSpeakers"]=json!(1.5);let r=h.post("/api/profiles/import",json!({"text":json!({"numeraProfiles":1,"profiles":[{"name":"valid-import","engine":"playwright"},bad]}).to_string()}),None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("mediaSpeakers"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_native_import_maxtouchpoints_1_5_352a90,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[native-import-maxTouchPoints-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["maxTouchPoints"]=json!(1.5);let r=h.post("/api/profiles/import",json!({"text":json!({"numeraProfiles":1,"profiles":[{"name":"valid-import","engine":"playwright"},bad]}).to_string()}),None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("maxTouchPoints"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_native_import_seed_1_5_3ce7f5,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[native-import-seed-1.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["seed"]=json!(1.5);let r=h.post("/api/profiles/import",json!({"text":json!({"numeraProfiles":1,"profiles":[{"name":"valid-import","engine":"playwright"},bad]}).to_string()}),None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("seed"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_every_http_write_rejects_a_fractional_count_atomically_native_import_memory_3_5_8cbfb3,"tests/test_dashboard_numeric_boundaries.py::test_every_http_write_rejects_a_fractional_count_atomically[native-import-memory-3.5]",{
let t=fresh();let root=t.path();let h=Http::new(root);
let a=h.create("existing-a");let b=h.create("existing-b");let _=(&a,&b);let before=snapshot(root);let mut bad=json!({"name":"invalid-new","engine":"playwright"});bad["memory"]=json!(3.5);let r=h.post("/api/profiles/import",json!({"text":json!({"numeraProfiles":1,"profiles":[{"name":"valid-import","engine":"playwright"},bad]}).to_string()}),None);assert!((400..500).contains(&r.status),"{}",r.text());assert!(r.text().contains("memory"),"{}",r.text());assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_direct_mirrored_store_cannot_bypass_numeric_policy_false_cores_1_5_9aa9af,"tests/test_dashboard_numeric_boundaries.py::test_direct_mirrored_store_cannot_bypass_numeric_policy[False-cores-1.5]",{
let t=fresh();let root=t.path();let store=numera_core::storage::dashboard::DashboardStore::new(root.to_owned(),None);let before=snapshot(root);let mut bad=json!({"id":"bad-number","name":"bad-number"});bad["cores"]=json!(1.5);let error=store.save(bad,Default::default(),None,None,1000.0).expect_err("fractional count must fail");assert!(error.to_string().contains("cores"));assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_direct_mirrored_store_cannot_bypass_numeric_policy_false_mediavideo_1_5_ca596c,"tests/test_dashboard_numeric_boundaries.py::test_direct_mirrored_store_cannot_bypass_numeric_policy[False-mediaVideo-1.5]",{
let t=fresh();let root=t.path();let store=numera_core::storage::dashboard::DashboardStore::new(root.to_owned(),None);let before=snapshot(root);let mut bad=json!({"id":"bad-number","name":"bad-number"});bad["mediaVideo"]=json!(1.5);let error=store.save(bad,Default::default(),None,None,1000.0).expect_err("fractional count must fail");assert!(error.to_string().contains("mediaVideo"));assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_direct_mirrored_store_cannot_bypass_numeric_policy_false_mediaaudio_1_5_1674d4,"tests/test_dashboard_numeric_boundaries.py::test_direct_mirrored_store_cannot_bypass_numeric_policy[False-mediaAudio-1.5]",{
let t=fresh();let root=t.path();let store=numera_core::storage::dashboard::DashboardStore::new(root.to_owned(),None);let before=snapshot(root);let mut bad=json!({"id":"bad-number","name":"bad-number"});bad["mediaAudio"]=json!(1.5);let error=store.save(bad,Default::default(),None,None,1000.0).expect_err("fractional count must fail");assert!(error.to_string().contains("mediaAudio"));assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_direct_mirrored_store_cannot_bypass_numeric_policy_false_mediaspeakers_1_5_f3ce5d,"tests/test_dashboard_numeric_boundaries.py::test_direct_mirrored_store_cannot_bypass_numeric_policy[False-mediaSpeakers-1.5]",{
let t=fresh();let root=t.path();let store=numera_core::storage::dashboard::DashboardStore::new(root.to_owned(),None);let before=snapshot(root);let mut bad=json!({"id":"bad-number","name":"bad-number"});bad["mediaSpeakers"]=json!(1.5);let error=store.save(bad,Default::default(),None,None,1000.0).expect_err("fractional count must fail");assert!(error.to_string().contains("mediaSpeakers"));assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_direct_mirrored_store_cannot_bypass_numeric_policy_false_maxtouchpoints_1_5_c96b53,"tests/test_dashboard_numeric_boundaries.py::test_direct_mirrored_store_cannot_bypass_numeric_policy[False-maxTouchPoints-1.5]",{
let t=fresh();let root=t.path();let store=numera_core::storage::dashboard::DashboardStore::new(root.to_owned(),None);let before=snapshot(root);let mut bad=json!({"id":"bad-number","name":"bad-number"});bad["maxTouchPoints"]=json!(1.5);let error=store.save(bad,Default::default(),None,None,1000.0).expect_err("fractional count must fail");assert!(error.to_string().contains("maxTouchPoints"));assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_direct_mirrored_store_cannot_bypass_numeric_policy_false_seed_1_5_3820a3,"tests/test_dashboard_numeric_boundaries.py::test_direct_mirrored_store_cannot_bypass_numeric_policy[False-seed-1.5]",{
let t=fresh();let root=t.path();let store=numera_core::storage::dashboard::DashboardStore::new(root.to_owned(),None);let before=snapshot(root);let mut bad=json!({"id":"bad-number","name":"bad-number"});bad["seed"]=json!(1.5);let error=store.save(bad,Default::default(),None,None,1000.0).expect_err("fractional count must fail");assert!(error.to_string().contains("seed"));assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_direct_mirrored_store_cannot_bypass_numeric_policy_false_memory_3_5_09d5ab,"tests/test_dashboard_numeric_boundaries.py::test_direct_mirrored_store_cannot_bypass_numeric_policy[False-memory-3.5]",{
let t=fresh();let root=t.path();let store=numera_core::storage::dashboard::DashboardStore::new(root.to_owned(),None);let before=snapshot(root);let mut bad=json!({"id":"bad-number","name":"bad-number"});bad["memory"]=json!(3.5);let error=store.save(bad,Default::default(),None,None,1000.0).expect_err("fractional count must fail");assert!(error.to_string().contains("memory"));assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_direct_mirrored_store_cannot_bypass_numeric_policy_true_cores_1_5_c43152,"tests/test_dashboard_numeric_boundaries.py::test_direct_mirrored_store_cannot_bypass_numeric_policy[True-cores-1.5]",{
let t=fresh();let root=t.path();let store=numera_core::storage::dashboard::DashboardStore::new(root.to_owned(),None);let before=snapshot(root);let mut bad=json!({"id":"bad-number","name":"bad-number"});bad["cores"]=json!(1.5);let error=store.save_many(vec![json!({"id":"good-number","name":"good-number"}),bad],Default::default(),None,1000.0).expect_err("fractional count must fail");assert!(error.to_string().contains("cores"));assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_direct_mirrored_store_cannot_bypass_numeric_policy_true_mediavideo_1_5_033039,"tests/test_dashboard_numeric_boundaries.py::test_direct_mirrored_store_cannot_bypass_numeric_policy[True-mediaVideo-1.5]",{
let t=fresh();let root=t.path();let store=numera_core::storage::dashboard::DashboardStore::new(root.to_owned(),None);let before=snapshot(root);let mut bad=json!({"id":"bad-number","name":"bad-number"});bad["mediaVideo"]=json!(1.5);let error=store.save_many(vec![json!({"id":"good-number","name":"good-number"}),bad],Default::default(),None,1000.0).expect_err("fractional count must fail");assert!(error.to_string().contains("mediaVideo"));assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_direct_mirrored_store_cannot_bypass_numeric_policy_true_mediaaudio_1_5_89551d,"tests/test_dashboard_numeric_boundaries.py::test_direct_mirrored_store_cannot_bypass_numeric_policy[True-mediaAudio-1.5]",{
let t=fresh();let root=t.path();let store=numera_core::storage::dashboard::DashboardStore::new(root.to_owned(),None);let before=snapshot(root);let mut bad=json!({"id":"bad-number","name":"bad-number"});bad["mediaAudio"]=json!(1.5);let error=store.save_many(vec![json!({"id":"good-number","name":"good-number"}),bad],Default::default(),None,1000.0).expect_err("fractional count must fail");assert!(error.to_string().contains("mediaAudio"));assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_direct_mirrored_store_cannot_bypass_numeric_policy_true_mediaspeakers_1_5_f49c67,"tests/test_dashboard_numeric_boundaries.py::test_direct_mirrored_store_cannot_bypass_numeric_policy[True-mediaSpeakers-1.5]",{
let t=fresh();let root=t.path();let store=numera_core::storage::dashboard::DashboardStore::new(root.to_owned(),None);let before=snapshot(root);let mut bad=json!({"id":"bad-number","name":"bad-number"});bad["mediaSpeakers"]=json!(1.5);let error=store.save_many(vec![json!({"id":"good-number","name":"good-number"}),bad],Default::default(),None,1000.0).expect_err("fractional count must fail");assert!(error.to_string().contains("mediaSpeakers"));assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_direct_mirrored_store_cannot_bypass_numeric_policy_true_maxtouchpoints_1_5_4185ee,"tests/test_dashboard_numeric_boundaries.py::test_direct_mirrored_store_cannot_bypass_numeric_policy[True-maxTouchPoints-1.5]",{
let t=fresh();let root=t.path();let store=numera_core::storage::dashboard::DashboardStore::new(root.to_owned(),None);let before=snapshot(root);let mut bad=json!({"id":"bad-number","name":"bad-number"});bad["maxTouchPoints"]=json!(1.5);let error=store.save_many(vec![json!({"id":"good-number","name":"good-number"}),bad],Default::default(),None,1000.0).expect_err("fractional count must fail");assert!(error.to_string().contains("maxTouchPoints"));assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_direct_mirrored_store_cannot_bypass_numeric_policy_true_seed_1_5_f41b1a,"tests/test_dashboard_numeric_boundaries.py::test_direct_mirrored_store_cannot_bypass_numeric_policy[True-seed-1.5]",{
let t=fresh();let root=t.path();let store=numera_core::storage::dashboard::DashboardStore::new(root.to_owned(),None);let before=snapshot(root);let mut bad=json!({"id":"bad-number","name":"bad-number"});bad["seed"]=json!(1.5);let error=store.save_many(vec![json!({"id":"good-number","name":"good-number"}),bad],Default::default(),None,1000.0).expect_err("fractional count must fail");assert!(error.to_string().contains("seed"));assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(dashboard_numeric_boundaries_test_direct_mirrored_store_cannot_bypass_numeric_policy_true_memory_3_5_94b615,"tests/test_dashboard_numeric_boundaries.py::test_direct_mirrored_store_cannot_bypass_numeric_policy[True-memory-3.5]",{
let t=fresh();let root=t.path();let store=numera_core::storage::dashboard::DashboardStore::new(root.to_owned(),None);let before=snapshot(root);let mut bad=json!({"id":"bad-number","name":"bad-number"});bad["memory"]=json!(3.5);let error=store.save_many(vec![json!({"id":"good-number","name":"good-number"}),bad],Default::default(),None,1000.0).expect_err("fractional count must fail");assert!(error.to_string().contains("memory"));assert_eq!(snapshot(root),before);
Ok(())
});
source_test!(
    dashboard_numeric_boundaries_test_http_allowed_numbers_match_both_persisted_copies_0_25_7c5606,
    "tests/test_dashboard_numeric_boundaries.py::test_http_allowed_numbers_match_both_persisted_copies[0.25]",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let r=h.post("/api/profiles",json!({"name":"precise","engine":"playwright","memory":0.25,"devicePixelRatio":1.5,"cores":8.0}),None);
        assert_eq!(r.status, 200, "{}", r.text());
        let id = text(&r.body, "id");
        let dash = read(root.join("dashboard").join(format!("{id}.json")));
        let engine = read(root.join("profiles").join(format!("{id}.json")));
        assert_eq!(dash["memory"].as_f64(), Some(0.25));
        assert_eq!(engine["fingerprint"]["device_memory"].as_f64(), Some(0.25));
        assert_eq!(dash["devicePixelRatio"].as_f64(), Some(1.5));
        assert_eq!(
            engine["fingerprint"]["device_pixel_ratio"].as_f64(),
            Some(1.5)
        );
        assert_eq!(dash["cores"].as_f64(), Some(8.0));
        assert_eq!(
            engine["fingerprint"]["hardware_concurrency"].as_f64(),
            Some(8.0)
        );
        let rows = h.get("/api/profiles", None).body;
        assert_eq!(
            rows.as_array()
                .expect("profile rows")
                .iter()
                .find(|row| row["id"] == id)
                .expect("new row")["memory"]
                .as_f64(),
            Some(0.25)
        );
        Ok(())
    }
);
source_test!(
    dashboard_numeric_boundaries_test_http_allowed_numbers_match_both_persisted_copies_0_5_95ec0f,
    "tests/test_dashboard_numeric_boundaries.py::test_http_allowed_numbers_match_both_persisted_copies[0.5]",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let r=h.post("/api/profiles",json!({"name":"precise","engine":"playwright","memory":0.5,"devicePixelRatio":1.5,"cores":8.0}),None);
        assert_eq!(r.status, 200, "{}", r.text());
        let id = text(&r.body, "id");
        let dash = read(root.join("dashboard").join(format!("{id}.json")));
        let engine = read(root.join("profiles").join(format!("{id}.json")));
        assert_eq!(dash["memory"].as_f64(), Some(0.5));
        assert_eq!(engine["fingerprint"]["device_memory"].as_f64(), Some(0.5));
        assert_eq!(dash["devicePixelRatio"].as_f64(), Some(1.5));
        assert_eq!(
            engine["fingerprint"]["device_pixel_ratio"].as_f64(),
            Some(1.5)
        );
        assert_eq!(dash["cores"].as_f64(), Some(8.0));
        assert_eq!(
            engine["fingerprint"]["hardware_concurrency"].as_f64(),
            Some(8.0)
        );
        let rows = h.get("/api/profiles", None).body;
        assert_eq!(
            rows.as_array()
                .expect("profile rows")
                .iter()
                .find(|row| row["id"] == id)
                .expect("new row")["memory"]
                .as_f64(),
            Some(0.5)
        );
        Ok(())
    }
);
source_test!(
    dashboard_numeric_boundaries_test_http_allowed_numbers_match_both_persisted_copies_1_05d204,
    "tests/test_dashboard_numeric_boundaries.py::test_http_allowed_numbers_match_both_persisted_copies[1]",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let r=h.post("/api/profiles",json!({"name":"precise","engine":"playwright","memory":1,"devicePixelRatio":1.5,"cores":8.0}),None);
        assert_eq!(r.status, 200, "{}", r.text());
        let id = text(&r.body, "id");
        let dash = read(root.join("dashboard").join(format!("{id}.json")));
        let engine = read(root.join("profiles").join(format!("{id}.json")));
        assert_eq!(dash["memory"].as_f64(), Some(1.0));
        assert_eq!(engine["fingerprint"]["device_memory"].as_f64(), Some(1.0));
        assert_eq!(dash["devicePixelRatio"].as_f64(), Some(1.5));
        assert_eq!(
            engine["fingerprint"]["device_pixel_ratio"].as_f64(),
            Some(1.5)
        );
        assert_eq!(dash["cores"].as_f64(), Some(8.0));
        assert_eq!(
            engine["fingerprint"]["hardware_concurrency"].as_f64(),
            Some(8.0)
        );
        let rows = h.get("/api/profiles", None).body;
        assert_eq!(
            rows.as_array()
                .expect("profile rows")
                .iter()
                .find(|row| row["id"] == id)
                .expect("new row")["memory"]
                .as_f64(),
            Some(1.0)
        );
        Ok(())
    }
);
source_test!(
    dashboard_numeric_boundaries_test_http_allowed_numbers_match_both_persisted_copies_16_4a6cc7,
    "tests/test_dashboard_numeric_boundaries.py::test_http_allowed_numbers_match_both_persisted_copies[16]",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let r=h.post("/api/profiles",json!({"name":"precise","engine":"playwright","memory":16,"devicePixelRatio":1.5,"cores":8.0}),None);
        assert_eq!(r.status, 200, "{}", r.text());
        let id = text(&r.body, "id");
        let dash = read(root.join("dashboard").join(format!("{id}.json")));
        let engine = read(root.join("profiles").join(format!("{id}.json")));
        assert_eq!(dash["memory"].as_f64(), Some(16.0));
        assert_eq!(engine["fingerprint"]["device_memory"].as_f64(), Some(16.0));
        assert_eq!(dash["devicePixelRatio"].as_f64(), Some(1.5));
        assert_eq!(
            engine["fingerprint"]["device_pixel_ratio"].as_f64(),
            Some(1.5)
        );
        assert_eq!(dash["cores"].as_f64(), Some(8.0));
        assert_eq!(
            engine["fingerprint"]["hardware_concurrency"].as_f64(),
            Some(8.0)
        );
        let rows = h.get("/api/profiles", None).body;
        assert_eq!(
            rows.as_array()
                .expect("profile rows")
                .iter()
                .find(|row| row["id"] == id)
                .expect("new row")["memory"]
                .as_f64(),
            Some(16.0)
        );
        Ok(())
    }
);
