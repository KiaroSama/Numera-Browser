use super::*;
fn unit_record(record: &Value) -> Value {
    json!({"pid":record["pid"],"pidToken":record["token"],"tree":record["tree"]})
}

source_test!(test_execute_preserves_child_environment_and_settles_only_its_direct_tree_normal_no_parent_e81835,"tests/test_native_entry_acceptance.py::test_execute_preserves_child_environment_and_settles_only_its_direct_tree[normal-no-parent]", {let mode="normal";let producer="emit";let reported=false;

let t=Temp::new()?;let h=helper()?;let child_report=receipt(t.path(),"deliberate-ancestor")?;let header=fs::read(&child_report)?;let parent_report=receipt(t.path(),"outer")?;
let selected=t.path().join("selected-env");let mut c=command(&h,producer);c.env("fixture","value").env(unit_report::ENV,&child_report);if producer=="emit"{c.arg(&selected);}
let before=c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>();let mut phases=Vec::new();let mut record=Value::Null;
let result=execute_observed(&mut c,None,None,|phase,tree|{phases.push(phase);if phase==ExecutePhase::Record{record=tree.clone();} Ok(())});
let expected=false;assert_eq!(result.is_err(),expected,"mode normal");if let Ok(result)=result{assert_eq!(result.exit,7);assert_eq!(result.stdout,"Fixture α");assert_eq!(result.stderr,"");}
let mut expected_phases=vec![ExecutePhase::Spawn,ExecutePhase::Record];if mode!="record"{expected_phases.push(ExecutePhase::Communicate);if mode!="communicate"{expected_phases.push(ExecutePhase::Decode);}}
expected_phases.push(ExecutePhase::Settle);if mode!="cleanup"{expected_phases.push(ExecutePhase::Wait);}expected_phases.push(ExecutePhase::Close);assert_eq!(phases,expected_phases);
assert_eq!(c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>(),before);assert_eq!(fs::read(&child_report)?,header);if !expected && producer=="emit" {assert_eq!(fs::read_to_string(selected)?,child_report.to_string_lossy());}
assert!(numera_core::ownership::tree::record_empty(&record)?);if reported {let report=unit_report::read(&parent_report)?;assert_eq!(report.complete,mode!="record");assert_eq!(report.trees.len(),usize::from(mode!="record"));if mode!="record"{assert_eq!(report.trees[0],unit_record(&record));}}

Ok(())
});
source_test!(test_execute_preserves_child_environment_and_settles_only_its_direct_tree_normal_parent_report_70f345,"tests/test_native_entry_acceptance.py::test_execute_preserves_child_environment_and_settles_only_its_direct_tree[normal-parent-report]", {let mode="normal";let producer="emit";let reported=true;

let t=Temp::new()?;let h=helper()?;let child_report=receipt(t.path(),"deliberate-ancestor")?;let header=fs::read(&child_report)?;let parent_report=receipt(t.path(),"outer")?;
let selected=t.path().join("selected-env");let mut c=command(&h,producer);c.env("fixture","value").env(unit_report::ENV,&child_report);if producer=="emit"{c.arg(&selected);}
let before=c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>();let mut phases=Vec::new();let mut record=Value::Null;
let result=execute_observed(&mut c,None,Some(parent_report.as_path()),|phase,tree|{phases.push(phase);if phase==ExecutePhase::Record{record=tree.clone();} Ok(())});
let expected=false;assert_eq!(result.is_err(),expected,"mode normal");if let Ok(result)=result{assert_eq!(result.exit,7);assert_eq!(result.stdout,"Fixture α");assert_eq!(result.stderr,"");}
let mut expected_phases=vec![ExecutePhase::Spawn,ExecutePhase::Record];if mode!="record"{expected_phases.push(ExecutePhase::Communicate);if mode!="communicate"{expected_phases.push(ExecutePhase::Decode);}}
expected_phases.push(ExecutePhase::Settle);if mode!="cleanup"{expected_phases.push(ExecutePhase::Wait);}expected_phases.push(ExecutePhase::Close);assert_eq!(phases,expected_phases);
assert_eq!(c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>(),before);assert_eq!(fs::read(&child_report)?,header);if !expected && producer=="emit" {assert_eq!(fs::read_to_string(selected)?,child_report.to_string_lossy());}
assert!(numera_core::ownership::tree::record_empty(&record)?);if reported {let report=unit_report::read(&parent_report)?;assert_eq!(report.complete,mode!="record");assert_eq!(report.trees.len(),usize::from(mode!="record"));if mode!="record"{assert_eq!(report.trees[0],unit_record(&record));}}

Ok(())
});
source_test!(test_execute_preserves_child_environment_and_settles_only_its_direct_tree_input_no_parent_3408ff,"tests/test_native_entry_acceptance.py::test_execute_preserves_child_environment_and_settles_only_its_direct_tree[input-no-parent]", {let mode="input";let producer="input";let reported=false;

let t=Temp::new()?;let h=helper()?;let child_report=receipt(t.path(),"deliberate-ancestor")?;let header=fs::read(&child_report)?;let parent_report=receipt(t.path(),"outer")?;
let selected=t.path().join("selected-env");let mut c=command(&h,producer);c.env("fixture","value").env(unit_report::ENV,&child_report);if producer=="emit"{c.arg(&selected);}
let before=c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>();let mut phases=Vec::new();let mut record=Value::Null;
let result=execute_observed(&mut c,Some(b"y\n".as_slice()),None,|phase,tree|{phases.push(phase);if phase==ExecutePhase::Record{record=tree.clone();} Ok(())});
let expected=false;assert_eq!(result.is_err(),expected,"mode input");if let Ok(result)=result{assert_eq!(result.exit,7);assert_eq!(result.stdout,"Fixture α");assert_eq!(result.stderr,"");}
let mut expected_phases=vec![ExecutePhase::Spawn,ExecutePhase::Record];if mode!="record"{expected_phases.push(ExecutePhase::Communicate);if mode!="communicate"{expected_phases.push(ExecutePhase::Decode);}}
expected_phases.push(ExecutePhase::Settle);if mode!="cleanup"{expected_phases.push(ExecutePhase::Wait);}expected_phases.push(ExecutePhase::Close);assert_eq!(phases,expected_phases);
assert_eq!(c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>(),before);assert_eq!(fs::read(&child_report)?,header);if !expected && producer=="emit" {assert_eq!(fs::read_to_string(selected)?,child_report.to_string_lossy());}
assert!(numera_core::ownership::tree::record_empty(&record)?);if reported {let report=unit_report::read(&parent_report)?;assert_eq!(report.complete,mode!="record");assert_eq!(report.trees.len(),usize::from(mode!="record"));if mode!="record"{assert_eq!(report.trees[0],unit_record(&record));}}

Ok(())
});
source_test!(test_execute_preserves_child_environment_and_settles_only_its_direct_tree_input_parent_report_8ab57c,"tests/test_native_entry_acceptance.py::test_execute_preserves_child_environment_and_settles_only_its_direct_tree[input-parent-report]", {let mode="input";let producer="input";let reported=true;

let t=Temp::new()?;let h=helper()?;let child_report=receipt(t.path(),"deliberate-ancestor")?;let header=fs::read(&child_report)?;let parent_report=receipt(t.path(),"outer")?;
let selected=t.path().join("selected-env");let mut c=command(&h,producer);c.env("fixture","value").env(unit_report::ENV,&child_report);if producer=="emit"{c.arg(&selected);}
let before=c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>();let mut phases=Vec::new();let mut record=Value::Null;
let result=execute_observed(&mut c,Some(b"y\n".as_slice()),Some(parent_report.as_path()),|phase,tree|{phases.push(phase);if phase==ExecutePhase::Record{record=tree.clone();} Ok(())});
let expected=false;assert_eq!(result.is_err(),expected,"mode input");if let Ok(result)=result{assert_eq!(result.exit,7);assert_eq!(result.stdout,"Fixture α");assert_eq!(result.stderr,"");}
let mut expected_phases=vec![ExecutePhase::Spawn,ExecutePhase::Record];if mode!="record"{expected_phases.push(ExecutePhase::Communicate);if mode!="communicate"{expected_phases.push(ExecutePhase::Decode);}}
expected_phases.push(ExecutePhase::Settle);if mode!="cleanup"{expected_phases.push(ExecutePhase::Wait);}expected_phases.push(ExecutePhase::Close);assert_eq!(phases,expected_phases);
assert_eq!(c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>(),before);assert_eq!(fs::read(&child_report)?,header);if !expected && producer=="emit" {assert_eq!(fs::read_to_string(selected)?,child_report.to_string_lossy());}
assert!(numera_core::ownership::tree::record_empty(&record)?);if reported {let report=unit_report::read(&parent_report)?;assert_eq!(report.complete,mode!="record");assert_eq!(report.trees.len(),usize::from(mode!="record"));if mode!="record"{assert_eq!(report.trees[0],unit_record(&record));}}

Ok(())
});
source_test!(test_execute_preserves_child_environment_and_settles_only_its_direct_tree_communicate_no_parent_ae0c3e,"tests/test_native_entry_acceptance.py::test_execute_preserves_child_environment_and_settles_only_its_direct_tree[communicate-no-parent]", {let mode="communicate";let producer="emit";let reported=false;

let t=Temp::new()?;let h=helper()?;let child_report=receipt(t.path(),"deliberate-ancestor")?;let header=fs::read(&child_report)?;let parent_report=receipt(t.path(),"outer")?;
let selected=t.path().join("selected-env");let mut c=command(&h,producer);c.env("fixture","value").env(unit_report::ENV,&child_report);if producer=="emit"{c.arg(&selected);}
let before=c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>();let mut phases=Vec::new();let mut record=Value::Null;
let result=execute_observed(&mut c,None,None,|phase,tree|{phases.push(phase);if phase==ExecutePhase::Record{record=tree.clone();}
if phase==ExecutePhase::Communicate{return Err(io::Error::new(io::ErrorKind::TimedOut,"controlled communication failure"));} Ok(())});
let expected=true;assert_eq!(result.is_err(),expected,"mode communicate");if let Ok(result)=result{assert_eq!(result.exit,7);assert_eq!(result.stdout,"Fixture α");assert_eq!(result.stderr,"");}
let mut expected_phases=vec![ExecutePhase::Spawn,ExecutePhase::Record];if mode!="record"{expected_phases.push(ExecutePhase::Communicate);if mode!="communicate"{expected_phases.push(ExecutePhase::Decode);}}
expected_phases.push(ExecutePhase::Settle);if mode!="cleanup"{expected_phases.push(ExecutePhase::Wait);}expected_phases.push(ExecutePhase::Close);assert_eq!(phases,expected_phases);
assert_eq!(c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>(),before);assert_eq!(fs::read(&child_report)?,header);if !expected && producer=="emit" {assert_eq!(fs::read_to_string(selected)?,child_report.to_string_lossy());}
assert!(numera_core::ownership::tree::record_empty(&record)?);if reported {let report=unit_report::read(&parent_report)?;assert_eq!(report.complete,mode!="record");assert_eq!(report.trees.len(),usize::from(mode!="record"));if mode!="record"{assert_eq!(report.trees[0],unit_record(&record));}}

Ok(())
});
source_test!(test_execute_preserves_child_environment_and_settles_only_its_direct_tree_communicate_parent_report_713274,"tests/test_native_entry_acceptance.py::test_execute_preserves_child_environment_and_settles_only_its_direct_tree[communicate-parent-report]", {let mode="communicate";let producer="emit";let reported=true;

let t=Temp::new()?;let h=helper()?;let child_report=receipt(t.path(),"deliberate-ancestor")?;let header=fs::read(&child_report)?;let parent_report=receipt(t.path(),"outer")?;
let selected=t.path().join("selected-env");let mut c=command(&h,producer);c.env("fixture","value").env(unit_report::ENV,&child_report);if producer=="emit"{c.arg(&selected);}
let before=c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>();let mut phases=Vec::new();let mut record=Value::Null;
let result=execute_observed(&mut c,None,Some(parent_report.as_path()),|phase,tree|{phases.push(phase);if phase==ExecutePhase::Record{record=tree.clone();}
if phase==ExecutePhase::Communicate{return Err(io::Error::new(io::ErrorKind::TimedOut,"controlled communication failure"));} Ok(())});
let expected=true;assert_eq!(result.is_err(),expected,"mode communicate");if let Ok(result)=result{assert_eq!(result.exit,7);assert_eq!(result.stdout,"Fixture α");assert_eq!(result.stderr,"");}
let mut expected_phases=vec![ExecutePhase::Spawn,ExecutePhase::Record];if mode!="record"{expected_phases.push(ExecutePhase::Communicate);if mode!="communicate"{expected_phases.push(ExecutePhase::Decode);}}
expected_phases.push(ExecutePhase::Settle);if mode!="cleanup"{expected_phases.push(ExecutePhase::Wait);}expected_phases.push(ExecutePhase::Close);assert_eq!(phases,expected_phases);
assert_eq!(c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>(),before);assert_eq!(fs::read(&child_report)?,header);if !expected && producer=="emit" {assert_eq!(fs::read_to_string(selected)?,child_report.to_string_lossy());}
assert!(numera_core::ownership::tree::record_empty(&record)?);if reported {let report=unit_report::read(&parent_report)?;assert_eq!(report.complete,mode!="record");assert_eq!(report.trees.len(),usize::from(mode!="record"));if mode!="record"{assert_eq!(report.trees[0],unit_record(&record));}}

Ok(())
});
source_test!(test_execute_preserves_child_environment_and_settles_only_its_direct_tree_decode_no_parent_f4c9e1,"tests/test_native_entry_acceptance.py::test_execute_preserves_child_environment_and_settles_only_its_direct_tree[decode-no-parent]", {let mode="decode";let producer="invalid";let reported=false;

let t=Temp::new()?;let h=helper()?;let child_report=receipt(t.path(),"deliberate-ancestor")?;let header=fs::read(&child_report)?;let parent_report=receipt(t.path(),"outer")?;
let selected=t.path().join("selected-env");let mut c=command(&h,producer);c.env("fixture","value").env(unit_report::ENV,&child_report);if producer=="emit"{c.arg(&selected);}
let before=c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>();let mut phases=Vec::new();let mut record=Value::Null;
let result=execute_observed(&mut c,None,None,|phase,tree|{phases.push(phase);if phase==ExecutePhase::Record{record=tree.clone();} Ok(())});
let expected=true;assert_eq!(result.is_err(),expected,"mode decode");if let Ok(result)=result{assert_eq!(result.exit,7);assert_eq!(result.stdout,"Fixture α");assert_eq!(result.stderr,"");}
let mut expected_phases=vec![ExecutePhase::Spawn,ExecutePhase::Record];if mode!="record"{expected_phases.push(ExecutePhase::Communicate);if mode!="communicate"{expected_phases.push(ExecutePhase::Decode);}}
expected_phases.push(ExecutePhase::Settle);if mode!="cleanup"{expected_phases.push(ExecutePhase::Wait);}expected_phases.push(ExecutePhase::Close);assert_eq!(phases,expected_phases);
assert_eq!(c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>(),before);assert_eq!(fs::read(&child_report)?,header);if !expected && producer=="emit" {assert_eq!(fs::read_to_string(selected)?,child_report.to_string_lossy());}
assert!(numera_core::ownership::tree::record_empty(&record)?);if reported {let report=unit_report::read(&parent_report)?;assert_eq!(report.complete,mode!="record");assert_eq!(report.trees.len(),usize::from(mode!="record"));if mode!="record"{assert_eq!(report.trees[0],unit_record(&record));}}

Ok(())
});
source_test!(test_execute_preserves_child_environment_and_settles_only_its_direct_tree_decode_parent_report_9358d2,"tests/test_native_entry_acceptance.py::test_execute_preserves_child_environment_and_settles_only_its_direct_tree[decode-parent-report]", {let mode="decode";let producer="invalid";let reported=true;

let t=Temp::new()?;let h=helper()?;let child_report=receipt(t.path(),"deliberate-ancestor")?;let header=fs::read(&child_report)?;let parent_report=receipt(t.path(),"outer")?;
let selected=t.path().join("selected-env");let mut c=command(&h,producer);c.env("fixture","value").env(unit_report::ENV,&child_report);if producer=="emit"{c.arg(&selected);}
let before=c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>();let mut phases=Vec::new();let mut record=Value::Null;
let result=execute_observed(&mut c,None,Some(parent_report.as_path()),|phase,tree|{phases.push(phase);if phase==ExecutePhase::Record{record=tree.clone();} Ok(())});
let expected=true;assert_eq!(result.is_err(),expected,"mode decode");if let Ok(result)=result{assert_eq!(result.exit,7);assert_eq!(result.stdout,"Fixture α");assert_eq!(result.stderr,"");}
let mut expected_phases=vec![ExecutePhase::Spawn,ExecutePhase::Record];if mode!="record"{expected_phases.push(ExecutePhase::Communicate);if mode!="communicate"{expected_phases.push(ExecutePhase::Decode);}}
expected_phases.push(ExecutePhase::Settle);if mode!="cleanup"{expected_phases.push(ExecutePhase::Wait);}expected_phases.push(ExecutePhase::Close);assert_eq!(phases,expected_phases);
assert_eq!(c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>(),before);assert_eq!(fs::read(&child_report)?,header);if !expected && producer=="emit" {assert_eq!(fs::read_to_string(selected)?,child_report.to_string_lossy());}
assert!(numera_core::ownership::tree::record_empty(&record)?);if reported {let report=unit_report::read(&parent_report)?;assert_eq!(report.complete,mode!="record");assert_eq!(report.trees.len(),usize::from(mode!="record"));if mode!="record"{assert_eq!(report.trees[0],unit_record(&record));}}

Ok(())
});
source_test!(test_execute_preserves_child_environment_and_settles_only_its_direct_tree_settlement_no_parent_a34bf7,"tests/test_native_entry_acceptance.py::test_execute_preserves_child_environment_and_settles_only_its_direct_tree[settlement-no-parent]", {let mode="settlement";let producer="emit";let reported=false;

let t=Temp::new()?;let h=helper()?;let child_report=receipt(t.path(),"deliberate-ancestor")?;let header=fs::read(&child_report)?;let parent_report=receipt(t.path(),"outer")?;
let selected=t.path().join("selected-env");let mut c=command(&h,producer);c.env("fixture","value").env(unit_report::ENV,&child_report);if producer=="emit"{c.arg(&selected);}
let before=c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>();let mut phases=Vec::new();let mut record=Value::Null;
let result=execute_observed(&mut c,None,None,|phase,tree|{phases.push(phase);if phase==ExecutePhase::Record{record=tree.clone();}
if phase==ExecutePhase::Wait{return Err(io::Error::other("Entry acceptance process tree did not settle"));} Ok(())});
let expected=true;assert_eq!(result.is_err(),expected,"mode settlement");if let Ok(result)=result{assert_eq!(result.exit,7);assert_eq!(result.stdout,"Fixture α");assert_eq!(result.stderr,"");}
let mut expected_phases=vec![ExecutePhase::Spawn,ExecutePhase::Record];if mode!="record"{expected_phases.push(ExecutePhase::Communicate);if mode!="communicate"{expected_phases.push(ExecutePhase::Decode);}}
expected_phases.push(ExecutePhase::Settle);if mode!="cleanup"{expected_phases.push(ExecutePhase::Wait);}expected_phases.push(ExecutePhase::Close);assert_eq!(phases,expected_phases);
assert_eq!(c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>(),before);assert_eq!(fs::read(&child_report)?,header);if !expected && producer=="emit" {assert_eq!(fs::read_to_string(selected)?,child_report.to_string_lossy());}
assert!(numera_core::ownership::tree::record_empty(&record)?);if reported {let report=unit_report::read(&parent_report)?;assert_eq!(report.complete,mode!="record");assert_eq!(report.trees.len(),usize::from(mode!="record"));if mode!="record"{assert_eq!(report.trees[0],unit_record(&record));}}

Ok(())
});
source_test!(test_execute_preserves_child_environment_and_settles_only_its_direct_tree_settlement_parent_report_8cb56c,"tests/test_native_entry_acceptance.py::test_execute_preserves_child_environment_and_settles_only_its_direct_tree[settlement-parent-report]", {let mode="settlement";let producer="emit";let reported=true;

let t=Temp::new()?;let h=helper()?;let child_report=receipt(t.path(),"deliberate-ancestor")?;let header=fs::read(&child_report)?;let parent_report=receipt(t.path(),"outer")?;
let selected=t.path().join("selected-env");let mut c=command(&h,producer);c.env("fixture","value").env(unit_report::ENV,&child_report);if producer=="emit"{c.arg(&selected);}
let before=c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>();let mut phases=Vec::new();let mut record=Value::Null;
let result=execute_observed(&mut c,None,Some(parent_report.as_path()),|phase,tree|{phases.push(phase);if phase==ExecutePhase::Record{record=tree.clone();}
if phase==ExecutePhase::Wait{return Err(io::Error::other("Entry acceptance process tree did not settle"));} Ok(())});
let expected=true;assert_eq!(result.is_err(),expected,"mode settlement");if let Ok(result)=result{assert_eq!(result.exit,7);assert_eq!(result.stdout,"Fixture α");assert_eq!(result.stderr,"");}
let mut expected_phases=vec![ExecutePhase::Spawn,ExecutePhase::Record];if mode!="record"{expected_phases.push(ExecutePhase::Communicate);if mode!="communicate"{expected_phases.push(ExecutePhase::Decode);}}
expected_phases.push(ExecutePhase::Settle);if mode!="cleanup"{expected_phases.push(ExecutePhase::Wait);}expected_phases.push(ExecutePhase::Close);assert_eq!(phases,expected_phases);
assert_eq!(c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>(),before);assert_eq!(fs::read(&child_report)?,header);if !expected && producer=="emit" {assert_eq!(fs::read_to_string(selected)?,child_report.to_string_lossy());}
assert!(numera_core::ownership::tree::record_empty(&record)?);if reported {let report=unit_report::read(&parent_report)?;assert_eq!(report.complete,mode!="record");assert_eq!(report.trees.len(),usize::from(mode!="record"));if mode!="record"{assert_eq!(report.trees[0],unit_record(&record));}}

Ok(())
});
source_test!(test_execute_preserves_child_environment_and_settles_only_its_direct_tree_record_no_parent_4fe440,"tests/test_native_entry_acceptance.py::test_execute_preserves_child_environment_and_settles_only_its_direct_tree[record-no-parent]", {let mode="record";let producer="emit";let reported=false;

let t=Temp::new()?;let h=helper()?;let child_report=receipt(t.path(),"deliberate-ancestor")?;let header=fs::read(&child_report)?;let parent_report=receipt(t.path(),"outer")?;
let selected=t.path().join("selected-env");let mut c=command(&h,producer);c.env("fixture","value").env(unit_report::ENV,&child_report);if producer=="emit"{c.arg(&selected);}
let before=c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>();let mut phases=Vec::new();let mut record=Value::Null;
let result=execute_observed(&mut c,None,None,|phase,tree|{phases.push(phase);if phase==ExecutePhase::Record{record=tree.clone();}
if phase==ExecutePhase::Record{return Err(io::Error::other("controlled receipt failure"));} Ok(())});
let expected=true;assert_eq!(result.is_err(),expected,"mode record");if let Ok(result)=result{assert_eq!(result.exit,7);assert_eq!(result.stdout,"Fixture α");assert_eq!(result.stderr,"");}
let mut expected_phases=vec![ExecutePhase::Spawn,ExecutePhase::Record];if mode!="record"{expected_phases.push(ExecutePhase::Communicate);if mode!="communicate"{expected_phases.push(ExecutePhase::Decode);}}
expected_phases.push(ExecutePhase::Settle);if mode!="cleanup"{expected_phases.push(ExecutePhase::Wait);}expected_phases.push(ExecutePhase::Close);assert_eq!(phases,expected_phases);
assert_eq!(c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>(),before);assert_eq!(fs::read(&child_report)?,header);if !expected && producer=="emit" {assert_eq!(fs::read_to_string(selected)?,child_report.to_string_lossy());}
assert!(numera_core::ownership::tree::record_empty(&record)?);if reported {let report=unit_report::read(&parent_report)?;assert_eq!(report.complete,mode!="record");assert_eq!(report.trees.len(),usize::from(mode!="record"));if mode!="record"{assert_eq!(report.trees[0],unit_record(&record));}}

Ok(())
});
source_test!(test_execute_preserves_child_environment_and_settles_only_its_direct_tree_record_parent_report_376c43,"tests/test_native_entry_acceptance.py::test_execute_preserves_child_environment_and_settles_only_its_direct_tree[record-parent-report]", {let mode="record";let producer="emit";let reported=true;

let t=Temp::new()?;let h=helper()?;let child_report=receipt(t.path(),"deliberate-ancestor")?;let header=fs::read(&child_report)?;let parent_report=receipt(t.path(),"outer")?;
let selected=t.path().join("selected-env");let mut c=command(&h,producer);c.env("fixture","value").env(unit_report::ENV,&child_report);if producer=="emit"{c.arg(&selected);}
let before=c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>();let mut phases=Vec::new();let mut record=Value::Null;
let result=execute_observed(&mut c,None,Some(parent_report.as_path()),|phase,tree|{phases.push(phase);if phase==ExecutePhase::Record{record=tree.clone();}
if phase==ExecutePhase::Record{return Err(io::Error::other("controlled receipt failure"));} Ok(())});
let expected=true;assert_eq!(result.is_err(),expected,"mode record");if let Ok(result)=result{assert_eq!(result.exit,7);assert_eq!(result.stdout,"Fixture α");assert_eq!(result.stderr,"");}
let mut expected_phases=vec![ExecutePhase::Spawn,ExecutePhase::Record];if mode!="record"{expected_phases.push(ExecutePhase::Communicate);if mode!="communicate"{expected_phases.push(ExecutePhase::Decode);}}
expected_phases.push(ExecutePhase::Settle);if mode!="cleanup"{expected_phases.push(ExecutePhase::Wait);}expected_phases.push(ExecutePhase::Close);assert_eq!(phases,expected_phases);
assert_eq!(c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>(),before);assert_eq!(fs::read(&child_report)?,header);if !expected && producer=="emit" {assert_eq!(fs::read_to_string(selected)?,child_report.to_string_lossy());}
assert!(numera_core::ownership::tree::record_empty(&record)?);if reported {let report=unit_report::read(&parent_report)?;assert_eq!(report.complete,mode!="record");assert_eq!(report.trees.len(),usize::from(mode!="record"));if mode!="record"{assert_eq!(report.trees[0],unit_record(&record));}}

Ok(())
});
source_test!(
    test_execute_preserves_child_environment_and_settles_only_its_direct_tree_wait_no_parent_a72199,
    "tests/test_native_entry_acceptance.py::test_execute_preserves_child_environment_and_settles_only_its_direct_tree[wait-no-parent]",
    {
        let mode = "wait";
        let producer = "emit";
        let reported = false;
        let t = Temp::new()?;
        let h = helper()?;
        let child_report = receipt(t.path(), "deliberate-ancestor")?;
        let header = fs::read(&child_report)?;
        let parent_report = receipt(t.path(), "outer")?;
        let selected = t.path().join("selected-env");
        let mut c = command(&h, producer);
        c.env("fixture", "value")
            .env(unit_report::ENV, &child_report);
        if producer == "emit" {
            c.arg(&selected);
        }
        let before = c
            .get_envs()
            .map(|(k, v)| (k.to_owned(), v.map(ToOwned::to_owned)))
            .collect::<BTreeMap<_, _>>();
        let mut phases = Vec::new();
        let mut record = Value::Null;
        let result = execute_observed(&mut c, None, None, |phase, tree| {
            phases.push(phase);
            if phase == ExecutePhase::Record {
                record = tree.clone();
            }
            if phase == ExecutePhase::Wait {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "controlled wait failure",
                ));
            }
            Ok(())
        });
        let expected = true;
        assert_eq!(result.is_err(), expected, "mode wait");
        if let Ok(result) = result {
            assert_eq!(result.exit, 7);
            assert_eq!(result.stdout, "Fixture α");
            assert_eq!(result.stderr, "");
        }
        let mut expected_phases = vec![ExecutePhase::Spawn, ExecutePhase::Record];
        if mode != "record" {
            expected_phases.push(ExecutePhase::Communicate);
            if mode != "communicate" {
                expected_phases.push(ExecutePhase::Decode);
            }
        }
        expected_phases.push(ExecutePhase::Settle);
        if mode != "cleanup" {
            expected_phases.push(ExecutePhase::Wait);
        }
        expected_phases.push(ExecutePhase::Close);
        assert_eq!(phases, expected_phases);
        assert_eq!(
            c.get_envs()
                .map(|(k, v)| (k.to_owned(), v.map(ToOwned::to_owned)))
                .collect::<BTreeMap<_, _>>(),
            before
        );
        assert_eq!(fs::read(&child_report)?, header);
        if !expected && producer == "emit" {
            assert_eq!(
                fs::read_to_string(selected)?,
                child_report.to_string_lossy()
            );
        }
        assert!(numera_core::ownership::tree::record_empty(&record)?);
        if reported {
            let report = unit_report::read(&parent_report)?;
            assert_eq!(report.complete, mode != "record");
            assert_eq!(report.trees.len(), usize::from(mode != "record"));
            if mode != "record" {
                assert_eq!(report.trees[0], unit_record(&record));
            }
        }

        Ok(())
    }
);
source_test!(test_execute_preserves_child_environment_and_settles_only_its_direct_tree_wait_parent_report_aacb43,"tests/test_native_entry_acceptance.py::test_execute_preserves_child_environment_and_settles_only_its_direct_tree[wait-parent-report]", {let mode="wait";let producer="emit";let reported=true;

let t=Temp::new()?;let h=helper()?;let child_report=receipt(t.path(),"deliberate-ancestor")?;let header=fs::read(&child_report)?;let parent_report=receipt(t.path(),"outer")?;
let selected=t.path().join("selected-env");let mut c=command(&h,producer);c.env("fixture","value").env(unit_report::ENV,&child_report);if producer=="emit"{c.arg(&selected);}
let before=c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>();let mut phases=Vec::new();let mut record=Value::Null;
let result=execute_observed(&mut c,None,Some(parent_report.as_path()),|phase,tree|{phases.push(phase);if phase==ExecutePhase::Record{record=tree.clone();}
if phase==ExecutePhase::Wait{return Err(io::Error::new(io::ErrorKind::TimedOut,"controlled wait failure"));} Ok(())});
let expected=true;assert_eq!(result.is_err(),expected,"mode wait");if let Ok(result)=result{assert_eq!(result.exit,7);assert_eq!(result.stdout,"Fixture α");assert_eq!(result.stderr,"");}
let mut expected_phases=vec![ExecutePhase::Spawn,ExecutePhase::Record];if mode!="record"{expected_phases.push(ExecutePhase::Communicate);if mode!="communicate"{expected_phases.push(ExecutePhase::Decode);}}
expected_phases.push(ExecutePhase::Settle);if mode!="cleanup"{expected_phases.push(ExecutePhase::Wait);}expected_phases.push(ExecutePhase::Close);assert_eq!(phases,expected_phases);
assert_eq!(c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>(),before);assert_eq!(fs::read(&child_report)?,header);if !expected && producer=="emit" {assert_eq!(fs::read_to_string(selected)?,child_report.to_string_lossy());}
assert!(numera_core::ownership::tree::record_empty(&record)?);if reported {let report=unit_report::read(&parent_report)?;assert_eq!(report.complete,mode!="record");assert_eq!(report.trees.len(),usize::from(mode!="record"));if mode!="record"{assert_eq!(report.trees[0],unit_record(&record));}}

Ok(())
});
source_test!(test_execute_preserves_child_environment_and_settles_only_its_direct_tree_cleanup_no_parent_6534d1,"tests/test_native_entry_acceptance.py::test_execute_preserves_child_environment_and_settles_only_its_direct_tree[cleanup-no-parent]", {let mode="cleanup";let producer="emit";let reported=false;

let t=Temp::new()?;let h=helper()?;let child_report=receipt(t.path(),"deliberate-ancestor")?;let header=fs::read(&child_report)?;let parent_report=receipt(t.path(),"outer")?;
let selected=t.path().join("selected-env");let mut c=command(&h,producer);c.env("fixture","value").env(unit_report::ENV,&child_report);if producer=="emit"{c.arg(&selected);}
let before=c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>();let mut phases=Vec::new();let mut record=Value::Null;
let result=execute_observed(&mut c,None,None,|phase,tree|{phases.push(phase);if phase==ExecutePhase::Record{record=tree.clone();}
if phase==ExecutePhase::Settle{return Err(io::Error::other("controlled cleanup query failure"));} Ok(())});
let expected=true;assert_eq!(result.is_err(),expected,"mode cleanup");if let Ok(result)=result{assert_eq!(result.exit,7);assert_eq!(result.stdout,"Fixture α");assert_eq!(result.stderr,"");}
let mut expected_phases=vec![ExecutePhase::Spawn,ExecutePhase::Record];if mode!="record"{expected_phases.push(ExecutePhase::Communicate);if mode!="communicate"{expected_phases.push(ExecutePhase::Decode);}}
expected_phases.push(ExecutePhase::Settle);if mode!="cleanup"{expected_phases.push(ExecutePhase::Wait);}expected_phases.push(ExecutePhase::Close);assert_eq!(phases,expected_phases);
assert_eq!(c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>(),before);assert_eq!(fs::read(&child_report)?,header);if !expected && producer=="emit" {assert_eq!(fs::read_to_string(selected)?,child_report.to_string_lossy());}
assert!(numera_core::ownership::tree::record_empty(&record)?);if reported {let report=unit_report::read(&parent_report)?;assert_eq!(report.complete,mode!="record");assert_eq!(report.trees.len(),usize::from(mode!="record"));if mode!="record"{assert_eq!(report.trees[0],unit_record(&record));}}

Ok(())
});
source_test!(test_execute_preserves_child_environment_and_settles_only_its_direct_tree_cleanup_parent_report_6cc2d5,"tests/test_native_entry_acceptance.py::test_execute_preserves_child_environment_and_settles_only_its_direct_tree[cleanup-parent-report]", {let mode="cleanup";let producer="emit";let reported=true;

let t=Temp::new()?;let h=helper()?;let child_report=receipt(t.path(),"deliberate-ancestor")?;let header=fs::read(&child_report)?;let parent_report=receipt(t.path(),"outer")?;
let selected=t.path().join("selected-env");let mut c=command(&h,producer);c.env("fixture","value").env(unit_report::ENV,&child_report);if producer=="emit"{c.arg(&selected);}
let before=c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>();let mut phases=Vec::new();let mut record=Value::Null;
let result=execute_observed(&mut c,None,Some(parent_report.as_path()),|phase,tree|{phases.push(phase);if phase==ExecutePhase::Record{record=tree.clone();}
if phase==ExecutePhase::Settle{return Err(io::Error::other("controlled cleanup query failure"));} Ok(())});
let expected=true;assert_eq!(result.is_err(),expected,"mode cleanup");if let Ok(result)=result{assert_eq!(result.exit,7);assert_eq!(result.stdout,"Fixture α");assert_eq!(result.stderr,"");}
let mut expected_phases=vec![ExecutePhase::Spawn,ExecutePhase::Record];if mode!="record"{expected_phases.push(ExecutePhase::Communicate);if mode!="communicate"{expected_phases.push(ExecutePhase::Decode);}}
expected_phases.push(ExecutePhase::Settle);if mode!="cleanup"{expected_phases.push(ExecutePhase::Wait);}expected_phases.push(ExecutePhase::Close);assert_eq!(phases,expected_phases);
assert_eq!(c.get_envs().map(|(k,v)|(k.to_owned(),v.map(ToOwned::to_owned))).collect::<BTreeMap<_,_>>(),before);assert_eq!(fs::read(&child_report)?,header);if !expected && producer=="emit" {assert_eq!(fs::read_to_string(selected)?,child_report.to_string_lossy());}
assert!(numera_core::ownership::tree::record_empty(&record)?);if reported {let report=unit_report::read(&parent_report)?;assert_eq!(report.complete,mode!="record");assert_eq!(report.trees.len(),usize::from(mode!="record"));if mode!="record"{assert_eq!(report.trees[0],unit_record(&record));}}

Ok(())
});
source_test!(
    test_main_removes_only_successful_entry_fixtures_false_no_parent_3b575e,
    "tests/test_native_entry_acceptance.py::test_main_removes_only_successful_entry_fixtures[False-no-parent]",
    {
        let failure = false;
        let reported = false;
        let t = Temp::new()?;
        let h = helper()?;
        let owned = t.path().join("owned-entry");
        let fixture = Fixture::create(&owned)?;
        let marker = owned.join("owner.json");
        fs::write(&marker, b"owned evidence")?;
        let parent = receipt(t.path(), "outer")?;
        let ancestor = fs::read(&parent)?;
        let env_path = owned.join("child-env");
        let mut c = command(&h, "json");
        c.arg(&env_path);
        if reported {
            c.env(unit_report::ENV, &parent);
        }
        let run = execute_observed(&mut c, None, None, |phase, _| {
            if failure && phase == ExecutePhase::Record {
                return Err(io::Error::other("Synthetic unsettled entry"));
            }
            Ok(())
        });
        let result = fixture.finish(run);
        if failure {
            assert!(result.is_err());
            assert_eq!(fs::read(marker)?, b"owned evidence");
        } else {
            assert_eq!(result?.exit, 0);
            assert!(!owned.exists());
        }
        assert_eq!(fs::read(parent)?, ancestor);

        Ok(())
    }
);
source_test!(
    test_main_removes_only_successful_entry_fixtures_false_parent_report_846211,
    "tests/test_native_entry_acceptance.py::test_main_removes_only_successful_entry_fixtures[False-parent-report]",
    {
        let failure = false;
        let reported = true;
        let t = Temp::new()?;
        let h = helper()?;
        let owned = t.path().join("owned-entry");
        let fixture = Fixture::create(&owned)?;
        let marker = owned.join("owner.json");
        fs::write(&marker, b"owned evidence")?;
        let parent = receipt(t.path(), "outer")?;
        let ancestor = fs::read(&parent)?;
        let env_path = owned.join("child-env");
        let mut c = command(&h, "json");
        c.arg(&env_path);
        if reported {
            c.env(unit_report::ENV, &parent);
        }
        let run = execute_observed(&mut c, None, None, |phase, _| {
            if failure && phase == ExecutePhase::Record {
                return Err(io::Error::other("Synthetic unsettled entry"));
            }
            Ok(())
        });
        let result = fixture.finish(run);
        if failure {
            assert!(result.is_err());
            assert_eq!(fs::read(marker)?, b"owned evidence");
        } else {
            assert_eq!(result?.exit, 0);
            assert!(!owned.exists());
        }
        assert_eq!(fs::read(parent)?, ancestor);

        Ok(())
    }
);
source_test!(
    test_main_removes_only_successful_entry_fixtures_true_no_parent_80bdbd,
    "tests/test_native_entry_acceptance.py::test_main_removes_only_successful_entry_fixtures[True-no-parent]",
    {
        let failure = true;
        let reported = false;
        let t = Temp::new()?;
        let h = helper()?;
        let owned = t.path().join("owned-entry");
        let fixture = Fixture::create(&owned)?;
        let marker = owned.join("owner.json");
        fs::write(&marker, b"owned evidence")?;
        let parent = receipt(t.path(), "outer")?;
        let ancestor = fs::read(&parent)?;
        let env_path = owned.join("child-env");
        let mut c = command(&h, "json");
        c.arg(&env_path);
        if reported {
            c.env(unit_report::ENV, &parent);
        }
        let run = execute_observed(&mut c, None, None, |phase, _| {
            if failure && phase == ExecutePhase::Record {
                return Err(io::Error::other("Synthetic unsettled entry"));
            }
            Ok(())
        });
        let result = fixture.finish(run);
        if failure {
            assert!(result.is_err());
            assert_eq!(fs::read(marker)?, b"owned evidence");
        } else {
            assert_eq!(result?.exit, 0);
            assert!(!owned.exists());
        }
        assert_eq!(fs::read(parent)?, ancestor);

        Ok(())
    }
);
source_test!(
    test_main_removes_only_successful_entry_fixtures_true_parent_report_e2ab80,
    "tests/test_native_entry_acceptance.py::test_main_removes_only_successful_entry_fixtures[True-parent-report]",
    {
        let failure = true;
        let reported = true;
        let t = Temp::new()?;
        let h = helper()?;
        let owned = t.path().join("owned-entry");
        let fixture = Fixture::create(&owned)?;
        let marker = owned.join("owner.json");
        fs::write(&marker, b"owned evidence")?;
        let parent = receipt(t.path(), "outer")?;
        let ancestor = fs::read(&parent)?;
        let env_path = owned.join("child-env");
        let mut c = command(&h, "json");
        c.arg(&env_path);
        if reported {
            c.env(unit_report::ENV, &parent);
        }
        let run = execute_observed(&mut c, None, None, |phase, _| {
            if failure && phase == ExecutePhase::Record {
                return Err(io::Error::other("Synthetic unsettled entry"));
            }
            Ok(())
        });
        let result = fixture.finish(run);
        if failure {
            assert!(result.is_err());
            assert_eq!(fs::read(marker)?, b"owned evidence");
        } else {
            assert_eq!(result?.exit, 0);
            assert!(!owned.exists());
        }
        assert_eq!(fs::read(parent)?, ancestor);

        Ok(())
    }
);
source_test!(
    test_detached_sdk_receipts_match_actual_command_and_session_claim_none_b69438,
    "tests/test_native_entry_acceptance.py::test_detached_sdk_receipts_match_actual_command_and_session_claim[None]",
    {
        let damage = "None";
        let t = Temp::new()?;
        let root = t.path();
        let h = helper()?;
        let mut command_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut sdk_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut foreign_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let command_record = command_tree.record();
        let sdk_record = sdk_tree.record();
        let foreign_record = foreign_tree.record();
        assert_ne!(foreign_record["pid"], command_record["pid"]);
        let mut job = command_record.clone();
        job["pidToken"] = command_record["token"].clone();
        job["id"] = json!("fixture-job");
        let claim =
            json!({"pid":sdk_record["pid"],"token":sdk_record["token"],"tree":sdk_record["tree"]});
        let trees = vec![command_record.clone(), sdk_record.clone()];

        let receipt = receipt(root, "fixture-job")?;
        for tree in &trees {
            let attempt = unit_report::begin_at(&receipt)?;
            unit_report::record_at(&receipt, tree, &attempt)?;
        }
        if damage == "pending" {
            let _ = unit_report::begin_at(&receipt)?;
        }
        if damage == "malformed" {
            let mut rows = fs::read_to_string(&receipt)?
                .lines()
                .map(serde_json::from_str::<Value>)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows.last_mut().ok_or("receipt empty")?["pidToken"] = Value::Null;
            fs::write(
                &receipt,
                rows.iter().map(|v| format!("{v}\n")).collect::<String>(),
            )?;
        }
        fs::create_dir_all(root.join("sessions"))?;
        let claim_path = root.join("sessions/fixture-profile.json");
        fs::write(&claim_path, serde_json::to_vec(&claim)?)?;
        let before = (fs::read(&receipt)?, fs::read(&claim_path)?);
        let mut observed = Vec::new();
        let result = sdk_units_with(root, &job, "fixture-profile", |record| {
            observed.push(record.clone());
            numera_core::ownership::tree::record_empty(record).map(|empty| !empty)
        });
        let expected = matches!(damage, "None" | "metadata");
        assert_eq!(result.is_ok(), expected, "None");
        if expected {
            assert_eq!(result?, trees.iter().map(unit_record).collect::<Vec<_>>());
            assert_eq!(observed.len(), 1);
            assert_eq!(observed[0]["pid"], sdk_record["pid"]);
        }
        assert_eq!((fs::read(receipt)?, fs::read(claim_path)?), before);
        command_tree.settle(Duration::from_secs(3))?;
        sdk_tree.settle(Duration::from_secs(3))?;
        foreign_tree.settle(Duration::from_secs(3))?;

        Ok(())
    }
);
source_test!(
    test_detached_sdk_receipts_match_actual_command_and_session_claim_metadata_18544c,
    "tests/test_native_entry_acceptance.py::test_detached_sdk_receipts_match_actual_command_and_session_claim[metadata]",
    {
        let damage = "metadata";
        let t = Temp::new()?;
        let root = t.path();
        let h = helper()?;
        let mut command_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut sdk_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut foreign_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let command_record = command_tree.record();
        let sdk_record = sdk_tree.record();
        let foreign_record = foreign_tree.record();
        assert_ne!(foreign_record["pid"], command_record["pid"]);
        let mut job = command_record.clone();
        job["pidToken"] = command_record["token"].clone();
        job["id"] = json!("fixture-job");
        let claim =
            json!({"pid":sdk_record["pid"],"token":sdk_record["token"],"tree":sdk_record["tree"]});
        let mut trees = vec![command_record.clone(), sdk_record.clone()];
        trees.insert(1, foreign_record.clone());
        let receipt = receipt(root, "fixture-job")?;
        for tree in &trees {
            let attempt = unit_report::begin_at(&receipt)?;
            unit_report::record_at(&receipt, tree, &attempt)?;
        }
        if damage == "pending" {
            let _ = unit_report::begin_at(&receipt)?;
        }
        if damage == "malformed" {
            let mut rows = fs::read_to_string(&receipt)?
                .lines()
                .map(serde_json::from_str::<Value>)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows.last_mut().ok_or("receipt empty")?["pidToken"] = Value::Null;
            fs::write(
                &receipt,
                rows.iter().map(|v| format!("{v}\n")).collect::<String>(),
            )?;
        }
        fs::create_dir_all(root.join("sessions"))?;
        let claim_path = root.join("sessions/fixture-profile.json");
        fs::write(&claim_path, serde_json::to_vec(&claim)?)?;
        let before = (fs::read(&receipt)?, fs::read(&claim_path)?);
        let mut observed = Vec::new();
        let result = sdk_units_with(root, &job, "fixture-profile", |record| {
            observed.push(record.clone());
            numera_core::ownership::tree::record_empty(record).map(|empty| !empty)
        });
        let expected = matches!(damage, "None" | "metadata");
        assert_eq!(result.is_ok(), expected, "metadata");
        if expected {
            assert_eq!(result?, trees.iter().map(unit_record).collect::<Vec<_>>());
            assert_eq!(observed.len(), 1);
            assert_eq!(observed[0]["pid"], sdk_record["pid"]);
        }
        assert_eq!((fs::read(receipt)?, fs::read(claim_path)?), before);
        command_tree.settle(Duration::from_secs(3))?;
        sdk_tree.settle(Duration::from_secs(3))?;
        foreign_tree.settle(Duration::from_secs(3))?;

        Ok(())
    }
);
source_test!(
    test_detached_sdk_receipts_match_actual_command_and_session_claim_missing_command_90e11b,
    "tests/test_native_entry_acceptance.py::test_detached_sdk_receipts_match_actual_command_and_session_claim[missing-command]",
    {
        let damage = "missing-command";
        let t = Temp::new()?;
        let root = t.path();
        let h = helper()?;
        let mut command_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut sdk_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut foreign_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let command_record = command_tree.record();
        let sdk_record = sdk_tree.record();
        let foreign_record = foreign_tree.record();
        assert_ne!(foreign_record["pid"], command_record["pid"]);
        let mut job = command_record.clone();
        job["pidToken"] = command_record["token"].clone();
        job["id"] = json!("fixture-job");
        let claim =
            json!({"pid":sdk_record["pid"],"token":sdk_record["token"],"tree":sdk_record["tree"]});
        let mut trees = vec![command_record.clone(), sdk_record.clone()];
        trees.remove(0);
        let receipt = receipt(root, "fixture-job")?;
        for tree in &trees {
            let attempt = unit_report::begin_at(&receipt)?;
            unit_report::record_at(&receipt, tree, &attempt)?;
        }
        if damage == "pending" {
            let _ = unit_report::begin_at(&receipt)?;
        }
        if damage == "malformed" {
            let mut rows = fs::read_to_string(&receipt)?
                .lines()
                .map(serde_json::from_str::<Value>)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows.last_mut().ok_or("receipt empty")?["pidToken"] = Value::Null;
            fs::write(
                &receipt,
                rows.iter().map(|v| format!("{v}\n")).collect::<String>(),
            )?;
        }
        fs::create_dir_all(root.join("sessions"))?;
        let claim_path = root.join("sessions/fixture-profile.json");
        fs::write(&claim_path, serde_json::to_vec(&claim)?)?;
        let before = (fs::read(&receipt)?, fs::read(&claim_path)?);
        let mut observed = Vec::new();
        let result = sdk_units_with(root, &job, "fixture-profile", |record| {
            observed.push(record.clone());
            numera_core::ownership::tree::record_empty(record).map(|empty| !empty)
        });
        let expected = matches!(damage, "None" | "metadata");
        assert_eq!(result.is_ok(), expected, "missing-command");
        if expected {
            assert_eq!(result?, trees.iter().map(unit_record).collect::<Vec<_>>());
            assert_eq!(observed.len(), 1);
            assert_eq!(observed[0]["pid"], sdk_record["pid"]);
        }
        assert_eq!((fs::read(receipt)?, fs::read(claim_path)?), before);
        command_tree.settle(Duration::from_secs(3))?;
        sdk_tree.settle(Duration::from_secs(3))?;
        foreign_tree.settle(Duration::from_secs(3))?;

        Ok(())
    }
);
source_test!(
    test_detached_sdk_receipts_match_actual_command_and_session_claim_missing_sdk_211d54,
    "tests/test_native_entry_acceptance.py::test_detached_sdk_receipts_match_actual_command_and_session_claim[missing-sdk]",
    {
        let damage = "missing-sdk";
        let t = Temp::new()?;
        let root = t.path();
        let h = helper()?;
        let mut command_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut sdk_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut foreign_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let command_record = command_tree.record();
        let sdk_record = sdk_tree.record();
        let foreign_record = foreign_tree.record();
        assert_ne!(foreign_record["pid"], command_record["pid"]);
        let mut job = command_record.clone();
        job["pidToken"] = command_record["token"].clone();
        job["id"] = json!("fixture-job");
        let claim =
            json!({"pid":sdk_record["pid"],"token":sdk_record["token"],"tree":sdk_record["tree"]});
        let mut trees = vec![command_record.clone(), sdk_record.clone()];
        trees.pop();
        let receipt = receipt(root, "fixture-job")?;
        for tree in &trees {
            let attempt = unit_report::begin_at(&receipt)?;
            unit_report::record_at(&receipt, tree, &attempt)?;
        }
        if damage == "pending" {
            let _ = unit_report::begin_at(&receipt)?;
        }
        if damage == "malformed" {
            let mut rows = fs::read_to_string(&receipt)?
                .lines()
                .map(serde_json::from_str::<Value>)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows.last_mut().ok_or("receipt empty")?["pidToken"] = Value::Null;
            fs::write(
                &receipt,
                rows.iter().map(|v| format!("{v}\n")).collect::<String>(),
            )?;
        }
        fs::create_dir_all(root.join("sessions"))?;
        let claim_path = root.join("sessions/fixture-profile.json");
        fs::write(&claim_path, serde_json::to_vec(&claim)?)?;
        let before = (fs::read(&receipt)?, fs::read(&claim_path)?);
        let mut observed = Vec::new();
        let result = sdk_units_with(root, &job, "fixture-profile", |record| {
            observed.push(record.clone());
            numera_core::ownership::tree::record_empty(record).map(|empty| !empty)
        });
        let expected = matches!(damage, "None" | "metadata");
        assert_eq!(result.is_ok(), expected, "missing-sdk");
        if expected {
            assert_eq!(result?, trees.iter().map(unit_record).collect::<Vec<_>>());
            assert_eq!(observed.len(), 1);
            assert_eq!(observed[0]["pid"], sdk_record["pid"]);
        }
        assert_eq!((fs::read(receipt)?, fs::read(claim_path)?), before);
        command_tree.settle(Duration::from_secs(3))?;
        sdk_tree.settle(Duration::from_secs(3))?;
        foreign_tree.settle(Duration::from_secs(3))?;

        Ok(())
    }
);
source_test!(
    test_detached_sdk_receipts_match_actual_command_and_session_claim_foreign_sdk_2a3067,
    "tests/test_native_entry_acceptance.py::test_detached_sdk_receipts_match_actual_command_and_session_claim[foreign-sdk]",
    {
        let damage = "foreign-sdk";
        let t = Temp::new()?;
        let root = t.path();
        let h = helper()?;
        let mut command_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut sdk_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut foreign_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let command_record = command_tree.record();
        let sdk_record = sdk_tree.record();
        let foreign_record = foreign_tree.record();
        assert_ne!(foreign_record["pid"], command_record["pid"]);
        let mut job = command_record.clone();
        job["pidToken"] = command_record["token"].clone();
        job["id"] = json!("fixture-job");
        let claim =
            json!({"pid":sdk_record["pid"],"token":sdk_record["token"],"tree":sdk_record["tree"]});
        let mut trees = vec![command_record.clone(), sdk_record.clone()];
        trees[1] = foreign_record.clone();
        let receipt = receipt(root, "fixture-job")?;
        for tree in &trees {
            let attempt = unit_report::begin_at(&receipt)?;
            unit_report::record_at(&receipt, tree, &attempt)?;
        }
        if damage == "pending" {
            let _ = unit_report::begin_at(&receipt)?;
        }
        if damage == "malformed" {
            let mut rows = fs::read_to_string(&receipt)?
                .lines()
                .map(serde_json::from_str::<Value>)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows.last_mut().ok_or("receipt empty")?["pidToken"] = Value::Null;
            fs::write(
                &receipt,
                rows.iter().map(|v| format!("{v}\n")).collect::<String>(),
            )?;
        }
        fs::create_dir_all(root.join("sessions"))?;
        let claim_path = root.join("sessions/fixture-profile.json");
        fs::write(&claim_path, serde_json::to_vec(&claim)?)?;
        let before = (fs::read(&receipt)?, fs::read(&claim_path)?);
        let mut observed = Vec::new();
        let result = sdk_units_with(root, &job, "fixture-profile", |record| {
            observed.push(record.clone());
            numera_core::ownership::tree::record_empty(record).map(|empty| !empty)
        });
        let expected = matches!(damage, "None" | "metadata");
        assert_eq!(result.is_ok(), expected, "foreign-sdk");
        if expected {
            assert_eq!(result?, trees.iter().map(unit_record).collect::<Vec<_>>());
            assert_eq!(observed.len(), 1);
            assert_eq!(observed[0]["pid"], sdk_record["pid"]);
        }
        assert_eq!((fs::read(receipt)?, fs::read(claim_path)?), before);
        command_tree.settle(Duration::from_secs(3))?;
        sdk_tree.settle(Duration::from_secs(3))?;
        foreign_tree.settle(Duration::from_secs(3))?;

        Ok(())
    }
);
source_test!(
    test_detached_sdk_receipts_match_actual_command_and_session_claim_same_command_8ae3e9,
    "tests/test_native_entry_acceptance.py::test_detached_sdk_receipts_match_actual_command_and_session_claim[same-command]",
    {
        let damage = "same-command";
        let t = Temp::new()?;
        let root = t.path();
        let h = helper()?;
        let mut command_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut sdk_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut foreign_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let command_record = command_tree.record();
        let sdk_record = sdk_tree.record();
        let foreign_record = foreign_tree.record();
        assert_ne!(foreign_record["pid"], command_record["pid"]);
        let mut job = command_record.clone();
        job["pidToken"] = command_record["token"].clone();
        job["id"] = json!("fixture-job");
        let mut claim =
            json!({"pid":sdk_record["pid"],"token":sdk_record["token"],"tree":sdk_record["tree"]});
        let trees = vec![command_record.clone(), sdk_record.clone()];
        claim = json!({"pid":command_record["pid"],"token":command_record["token"],"tree":command_record["tree"]});
        let receipt = receipt(root, "fixture-job")?;
        for tree in &trees {
            let attempt = unit_report::begin_at(&receipt)?;
            unit_report::record_at(&receipt, tree, &attempt)?;
        }
        if damage == "pending" {
            let _ = unit_report::begin_at(&receipt)?;
        }
        if damage == "malformed" {
            let mut rows = fs::read_to_string(&receipt)?
                .lines()
                .map(serde_json::from_str::<Value>)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows.last_mut().ok_or("receipt empty")?["pidToken"] = Value::Null;
            fs::write(
                &receipt,
                rows.iter().map(|v| format!("{v}\n")).collect::<String>(),
            )?;
        }
        fs::create_dir_all(root.join("sessions"))?;
        let claim_path = root.join("sessions/fixture-profile.json");
        fs::write(&claim_path, serde_json::to_vec(&claim)?)?;
        let before = (fs::read(&receipt)?, fs::read(&claim_path)?);
        let mut observed = Vec::new();
        let result = sdk_units_with(root, &job, "fixture-profile", |record| {
            observed.push(record.clone());
            numera_core::ownership::tree::record_empty(record).map(|empty| !empty)
        });
        let expected = matches!(damage, "None" | "metadata");
        assert_eq!(result.is_ok(), expected, "same-command");
        if expected {
            assert_eq!(result?, trees.iter().map(unit_record).collect::<Vec<_>>());
            assert_eq!(observed.len(), 1);
            assert_eq!(observed[0]["pid"], sdk_record["pid"]);
        }
        assert_eq!((fs::read(receipt)?, fs::read(claim_path)?), before);
        command_tree.settle(Duration::from_secs(3))?;
        sdk_tree.settle(Duration::from_secs(3))?;
        foreign_tree.settle(Duration::from_secs(3))?;

        Ok(())
    }
);
source_test!(
    test_detached_sdk_receipts_match_actual_command_and_session_claim_command_token_115c1b,
    "tests/test_native_entry_acceptance.py::test_detached_sdk_receipts_match_actual_command_and_session_claim[command-token]",
    {
        let damage = "command-token";
        let t = Temp::new()?;
        let root = t.path();
        let h = helper()?;
        let mut command_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut sdk_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut foreign_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let command_record = command_tree.record();
        let sdk_record = sdk_tree.record();
        let foreign_record = foreign_tree.record();
        assert_ne!(foreign_record["pid"], command_record["pid"]);
        let mut job = command_record.clone();
        job["pidToken"] = command_record["token"].clone();
        job["id"] = json!("fixture-job");
        let claim =
            json!({"pid":sdk_record["pid"],"token":sdk_record["token"],"tree":sdk_record["tree"]});
        let trees = vec![command_record.clone(), sdk_record.clone()];
        job["pidToken"] = json!(job["pidToken"].as_u64().ok_or("command token missing")? + 1);
        let receipt = receipt(root, "fixture-job")?;
        for tree in &trees {
            let attempt = unit_report::begin_at(&receipt)?;
            unit_report::record_at(&receipt, tree, &attempt)?;
        }
        if damage == "pending" {
            let _ = unit_report::begin_at(&receipt)?;
        }
        if damage == "malformed" {
            let mut rows = fs::read_to_string(&receipt)?
                .lines()
                .map(serde_json::from_str::<Value>)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows.last_mut().ok_or("receipt empty")?["pidToken"] = Value::Null;
            fs::write(
                &receipt,
                rows.iter().map(|v| format!("{v}\n")).collect::<String>(),
            )?;
        }
        fs::create_dir_all(root.join("sessions"))?;
        let claim_path = root.join("sessions/fixture-profile.json");
        fs::write(&claim_path, serde_json::to_vec(&claim)?)?;
        let before = (fs::read(&receipt)?, fs::read(&claim_path)?);
        let mut observed = Vec::new();
        let result = sdk_units_with(root, &job, "fixture-profile", |record| {
            observed.push(record.clone());
            numera_core::ownership::tree::record_empty(record).map(|empty| !empty)
        });
        let expected = matches!(damage, "None" | "metadata");
        assert_eq!(result.is_ok(), expected, "command-token");
        if expected {
            assert_eq!(result?, trees.iter().map(unit_record).collect::<Vec<_>>());
            assert_eq!(observed.len(), 1);
            assert_eq!(observed[0]["pid"], sdk_record["pid"]);
        }
        assert_eq!((fs::read(receipt)?, fs::read(claim_path)?), before);
        command_tree.settle(Duration::from_secs(3))?;
        sdk_tree.settle(Duration::from_secs(3))?;
        foreign_tree.settle(Duration::from_secs(3))?;

        Ok(())
    }
);
source_test!(
    test_detached_sdk_receipts_match_actual_command_and_session_claim_sdk_token_bbfb5d,
    "tests/test_native_entry_acceptance.py::test_detached_sdk_receipts_match_actual_command_and_session_claim[sdk-token]",
    {
        let damage = "sdk-token";
        let t = Temp::new()?;
        let root = t.path();
        let h = helper()?;
        let mut command_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut sdk_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut foreign_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let command_record = command_tree.record();
        let sdk_record = sdk_tree.record();
        let foreign_record = foreign_tree.record();
        assert_ne!(foreign_record["pid"], command_record["pid"]);
        let mut job = command_record.clone();
        job["pidToken"] = command_record["token"].clone();
        job["id"] = json!("fixture-job");
        let mut claim =
            json!({"pid":sdk_record["pid"],"token":sdk_record["token"],"tree":sdk_record["tree"]});
        let trees = vec![command_record.clone(), sdk_record.clone()];
        claim["token"] = json!(claim["token"].as_u64().ok_or("sdk token missing")? + 1);
        let receipt = receipt(root, "fixture-job")?;
        for tree in &trees {
            let attempt = unit_report::begin_at(&receipt)?;
            unit_report::record_at(&receipt, tree, &attempt)?;
        }
        if damage == "pending" {
            let _ = unit_report::begin_at(&receipt)?;
        }
        if damage == "malformed" {
            let mut rows = fs::read_to_string(&receipt)?
                .lines()
                .map(serde_json::from_str::<Value>)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows.last_mut().ok_or("receipt empty")?["pidToken"] = Value::Null;
            fs::write(
                &receipt,
                rows.iter().map(|v| format!("{v}\n")).collect::<String>(),
            )?;
        }
        fs::create_dir_all(root.join("sessions"))?;
        let claim_path = root.join("sessions/fixture-profile.json");
        fs::write(&claim_path, serde_json::to_vec(&claim)?)?;
        let before = (fs::read(&receipt)?, fs::read(&claim_path)?);
        let mut observed = Vec::new();
        let result = sdk_units_with(root, &job, "fixture-profile", |record| {
            observed.push(record.clone());
            numera_core::ownership::tree::record_empty(record).map(|empty| !empty)
        });
        let expected = matches!(damage, "None" | "metadata");
        assert_eq!(result.is_ok(), expected, "sdk-token");
        if expected {
            assert_eq!(result?, trees.iter().map(unit_record).collect::<Vec<_>>());
            assert_eq!(observed.len(), 1);
            assert_eq!(observed[0]["pid"], sdk_record["pid"]);
        }
        assert_eq!((fs::read(receipt)?, fs::read(claim_path)?), before);
        command_tree.settle(Duration::from_secs(3))?;
        sdk_tree.settle(Duration::from_secs(3))?;
        foreign_tree.settle(Duration::from_secs(3))?;

        Ok(())
    }
);
source_test!(
    test_detached_sdk_receipts_match_actual_command_and_session_claim_sdk_group_fab87f,
    "tests/test_native_entry_acceptance.py::test_detached_sdk_receipts_match_actual_command_and_session_claim[sdk-group]",
    {
        let damage = "sdk-group";
        let t = Temp::new()?;
        let root = t.path();
        let h = helper()?;
        let mut command_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut sdk_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut foreign_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let command_record = command_tree.record();
        let sdk_record = sdk_tree.record();
        let foreign_record = foreign_tree.record();
        assert_ne!(foreign_record["pid"], command_record["pid"]);
        let mut job = command_record.clone();
        job["pidToken"] = command_record["token"].clone();
        job["id"] = json!("fixture-job");
        let mut claim =
            json!({"pid":sdk_record["pid"],"token":sdk_record["token"],"tree":sdk_record["tree"]});
        let trees = vec![command_record.clone(), sdk_record.clone()];
        claim["tree"] = json!({"kind":"job","group":"different-owned-tree"});
        let receipt = receipt(root, "fixture-job")?;
        for tree in &trees {
            let attempt = unit_report::begin_at(&receipt)?;
            unit_report::record_at(&receipt, tree, &attempt)?;
        }
        if damage == "pending" {
            let _ = unit_report::begin_at(&receipt)?;
        }
        if damage == "malformed" {
            let mut rows = fs::read_to_string(&receipt)?
                .lines()
                .map(serde_json::from_str::<Value>)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows.last_mut().ok_or("receipt empty")?["pidToken"] = Value::Null;
            fs::write(
                &receipt,
                rows.iter().map(|v| format!("{v}\n")).collect::<String>(),
            )?;
        }
        fs::create_dir_all(root.join("sessions"))?;
        let claim_path = root.join("sessions/fixture-profile.json");
        fs::write(&claim_path, serde_json::to_vec(&claim)?)?;
        let before = (fs::read(&receipt)?, fs::read(&claim_path)?);
        let mut observed = Vec::new();
        let result = sdk_units_with(root, &job, "fixture-profile", |record| {
            observed.push(record.clone());
            numera_core::ownership::tree::record_empty(record).map(|empty| !empty)
        });
        let expected = matches!(damage, "None" | "metadata");
        assert_eq!(result.is_ok(), expected, "sdk-group");
        if expected {
            assert_eq!(result?, trees.iter().map(unit_record).collect::<Vec<_>>());
            assert_eq!(observed.len(), 1);
            assert_eq!(observed[0]["pid"], sdk_record["pid"]);
        }
        assert_eq!((fs::read(receipt)?, fs::read(claim_path)?), before);
        command_tree.settle(Duration::from_secs(3))?;
        sdk_tree.settle(Duration::from_secs(3))?;
        foreign_tree.settle(Duration::from_secs(3))?;

        Ok(())
    }
);
source_test!(
    test_detached_sdk_receipts_match_actual_command_and_session_claim_dead_sdk_181661,
    "tests/test_native_entry_acceptance.py::test_detached_sdk_receipts_match_actual_command_and_session_claim[dead-sdk]",
    {
        let damage = "dead-sdk";
        let t = Temp::new()?;
        let root = t.path();
        let h = helper()?;
        let mut command_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut sdk_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut foreign_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let command_record = command_tree.record();
        let sdk_record = sdk_tree.record();
        let foreign_record = foreign_tree.record();
        assert_ne!(foreign_record["pid"], command_record["pid"]);
        let mut job = command_record.clone();
        job["pidToken"] = command_record["token"].clone();
        job["id"] = json!("fixture-job");
        let claim =
            json!({"pid":sdk_record["pid"],"token":sdk_record["token"],"tree":sdk_record["tree"]});
        let trees = vec![command_record.clone(), sdk_record.clone()];
        sdk_tree.settle(Duration::from_secs(3))?;
        let receipt = receipt(root, "fixture-job")?;
        for tree in &trees {
            let attempt = unit_report::begin_at(&receipt)?;
            unit_report::record_at(&receipt, tree, &attempt)?;
        }
        if damage == "pending" {
            let _ = unit_report::begin_at(&receipt)?;
        }
        if damage == "malformed" {
            let mut rows = fs::read_to_string(&receipt)?
                .lines()
                .map(serde_json::from_str::<Value>)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows.last_mut().ok_or("receipt empty")?["pidToken"] = Value::Null;
            fs::write(
                &receipt,
                rows.iter().map(|v| format!("{v}\n")).collect::<String>(),
            )?;
        }
        fs::create_dir_all(root.join("sessions"))?;
        let claim_path = root.join("sessions/fixture-profile.json");
        fs::write(&claim_path, serde_json::to_vec(&claim)?)?;
        let before = (fs::read(&receipt)?, fs::read(&claim_path)?);
        let mut observed = Vec::new();
        let result = sdk_units_with(root, &job, "fixture-profile", |record| {
            observed.push(record.clone());
            numera_core::ownership::tree::record_empty(record).map(|empty| !empty)
        });
        let expected = matches!(damage, "None" | "metadata");
        assert_eq!(result.is_ok(), expected, "dead-sdk");
        if expected {
            assert_eq!(result?, trees.iter().map(unit_record).collect::<Vec<_>>());
            assert_eq!(observed.len(), 1);
            assert_eq!(observed[0]["pid"], sdk_record["pid"]);
        }
        assert_eq!((fs::read(receipt)?, fs::read(claim_path)?), before);
        command_tree.settle(Duration::from_secs(3))?;
        sdk_tree.settle(Duration::from_secs(3))?;
        foreign_tree.settle(Duration::from_secs(3))?;

        Ok(())
    }
);
source_test!(
    test_detached_sdk_receipts_match_actual_command_and_session_claim_pending_4e91ca,
    "tests/test_native_entry_acceptance.py::test_detached_sdk_receipts_match_actual_command_and_session_claim[pending]",
    {
        let damage = "pending";
        let t = Temp::new()?;
        let root = t.path();
        let h = helper()?;
        let mut command_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut sdk_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut foreign_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let command_record = command_tree.record();
        let sdk_record = sdk_tree.record();
        let foreign_record = foreign_tree.record();
        assert_ne!(foreign_record["pid"], command_record["pid"]);
        let mut job = command_record.clone();
        job["pidToken"] = command_record["token"].clone();
        job["id"] = json!("fixture-job");
        let claim =
            json!({"pid":sdk_record["pid"],"token":sdk_record["token"],"tree":sdk_record["tree"]});
        let trees = vec![command_record.clone(), sdk_record.clone()];

        let receipt = receipt(root, "fixture-job")?;
        for tree in &trees {
            let attempt = unit_report::begin_at(&receipt)?;
            unit_report::record_at(&receipt, tree, &attempt)?;
        }
        if damage == "pending" {
            let _ = unit_report::begin_at(&receipt)?;
        }
        if damage == "malformed" {
            let mut rows = fs::read_to_string(&receipt)?
                .lines()
                .map(serde_json::from_str::<Value>)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows.last_mut().ok_or("receipt empty")?["pidToken"] = Value::Null;
            fs::write(
                &receipt,
                rows.iter().map(|v| format!("{v}\n")).collect::<String>(),
            )?;
        }
        fs::create_dir_all(root.join("sessions"))?;
        let claim_path = root.join("sessions/fixture-profile.json");
        fs::write(&claim_path, serde_json::to_vec(&claim)?)?;
        let before = (fs::read(&receipt)?, fs::read(&claim_path)?);
        let mut observed = Vec::new();
        let result = sdk_units_with(root, &job, "fixture-profile", |record| {
            observed.push(record.clone());
            numera_core::ownership::tree::record_empty(record).map(|empty| !empty)
        });
        let expected = matches!(damage, "None" | "metadata");
        assert_eq!(result.is_ok(), expected, "pending");
        if expected {
            assert_eq!(result?, trees.iter().map(unit_record).collect::<Vec<_>>());
            assert_eq!(observed.len(), 1);
            assert_eq!(observed[0]["pid"], sdk_record["pid"]);
        }
        assert_eq!((fs::read(receipt)?, fs::read(claim_path)?), before);
        command_tree.settle(Duration::from_secs(3))?;
        sdk_tree.settle(Duration::from_secs(3))?;
        foreign_tree.settle(Duration::from_secs(3))?;

        Ok(())
    }
);
source_test!(
    test_detached_sdk_receipts_match_actual_command_and_session_claim_malformed_ce12bc,
    "tests/test_native_entry_acceptance.py::test_detached_sdk_receipts_match_actual_command_and_session_claim[malformed]",
    {
        let damage = "malformed";
        let t = Temp::new()?;
        let root = t.path();
        let h = helper()?;
        let mut command_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut sdk_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let mut foreign_tree = OwnedTree::spawn(&mut command(&h, "hold"))?;
        let command_record = command_tree.record();
        let sdk_record = sdk_tree.record();
        let foreign_record = foreign_tree.record();
        assert_ne!(foreign_record["pid"], command_record["pid"]);
        let mut job = command_record.clone();
        job["pidToken"] = command_record["token"].clone();
        job["id"] = json!("fixture-job");
        let claim =
            json!({"pid":sdk_record["pid"],"token":sdk_record["token"],"tree":sdk_record["tree"]});
        let trees = vec![command_record.clone(), sdk_record.clone()];

        let receipt = receipt(root, "fixture-job")?;
        for tree in &trees {
            let attempt = unit_report::begin_at(&receipt)?;
            unit_report::record_at(&receipt, tree, &attempt)?;
        }
        if damage == "pending" {
            let _ = unit_report::begin_at(&receipt)?;
        }
        if damage == "malformed" {
            let mut rows = fs::read_to_string(&receipt)?
                .lines()
                .map(serde_json::from_str::<Value>)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows.last_mut().ok_or("receipt empty")?["pidToken"] = Value::Null;
            fs::write(
                &receipt,
                rows.iter().map(|v| format!("{v}\n")).collect::<String>(),
            )?;
        }
        fs::create_dir_all(root.join("sessions"))?;
        let claim_path = root.join("sessions/fixture-profile.json");
        fs::write(&claim_path, serde_json::to_vec(&claim)?)?;
        let before = (fs::read(&receipt)?, fs::read(&claim_path)?);
        let mut observed = Vec::new();
        let result = sdk_units_with(root, &job, "fixture-profile", |record| {
            observed.push(record.clone());
            numera_core::ownership::tree::record_empty(record).map(|empty| !empty)
        });
        let expected = matches!(damage, "None" | "metadata");
        assert_eq!(result.is_ok(), expected, "malformed");
        if expected {
            assert_eq!(result?, trees.iter().map(unit_record).collect::<Vec<_>>());
            assert_eq!(observed.len(), 1);
            assert_eq!(observed[0]["pid"], sdk_record["pid"]);
        }
        assert_eq!((fs::read(receipt)?, fs::read(claim_path)?), before);
        command_tree.settle(Duration::from_secs(3))?;
        sdk_tree.settle(Duration::from_secs(3))?;
        foreign_tree.settle(Duration::from_secs(3))?;

        Ok(())
    }
);
