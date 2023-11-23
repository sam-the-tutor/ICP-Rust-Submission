#[macro_use]
extern crate serde;

use candid::Principal;
use candid::{Decode, Encode};

use validator::Validate;

use ic_cdk::api::caller;
use ic_cdk::api::time;

use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};

use ic_stable_structures::{BoundedStorable, Cell, DefaultMemoryImpl, StableBTreeMap, Storable};

use std::{borrow::Cow, cell::RefCell};

type Memory = VirtualMemory<DefaultMemoryImpl>;
type IdCell = Cell<u64, Memory>;

//admin principal ID
//replace it with your own admin principal
const ADMIN_PRINCIPAL_ID: &str = "2vxsx-fae";

//struct to store the info about the task
#[derive(candid::CandidType, Clone, Serialize, Deserialize, Default)]
struct TASK {
    id: u64,
    title: String,           //title of the task
    description: String,     //a brief description of the task
    assigned_to: String,     // the team member to whom it is assigned
    is_done: bool,           // to track whether the task has been completed
    start_time: u64,         //the time at which the task started
    deadline: u8,            //deadline in hours
    updated_at: Option<u64>, //update the task by the admin
}

#[derive(candid::CandidType, Serialize, Deserialize, Default, Validate)]
struct TASKPayload {
    #[validate(length(min = 3))]
    title: String,
    #[validate(length(min = 5))]
    description: String,
    assigned_to: String,
    #[validate(range(min = 1))]
    deadline: u8,
}

//struct to store the info about the members
#[derive(candid::CandidType, Clone, Serialize, Deserialize, Default)]
struct Member {
    id: u64,
    principal_id: String,
}

// a trait that must be implemented for a struct that is stored in a stable struct
impl Storable for Member {
    fn to_bytes(&self) -> std::borrow::Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }
    fn from_bytes(bytes: std::borrow::Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
}

// another trait that must be implemented for a struct that is stored in a stable struct
impl BoundedStorable for Member {
    const MAX_SIZE: u32 = 1024;

    const IS_FIXED_SIZE: bool = false;
}

// a trait that must be implemented for a struct that is stored in a stable struct
impl Storable for TASK {
    fn to_bytes(&self) -> std::borrow::Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }
    fn from_bytes(bytes: std::borrow::Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
}

// another trait that must be implemented for a struct that is stored in a stable struct
impl BoundedStorable for TASK {
    const MAX_SIZE: u32 = 1024;
    const IS_FIXED_SIZE: bool = false;
}

thread_local! {
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(
     MemoryManager::init(DefaultMemoryImpl::default())
    );
    static ID_COUNTER: RefCell<IdCell> = RefCell::new(
        IdCell::init(MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(0))), 0)
        .expect("Cannot create a counter")
    );

    static TASKS: RefCell<StableBTreeMap<u64, TASK, Memory>> =
        RefCell::new(StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(1)))
    ));
    static MEMBERS: RefCell<StableBTreeMap<u64, Member, Memory>> =
    RefCell::new(StableBTreeMap::init(
    MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(2)))

));
    static ID_COUNT: RefCell<IdCell> = RefCell::new(
        IdCell::init(MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(3))), 0)
            .expect("Cannot create a counter")
    );
}

//get the member using the member id
#[ic_cdk::query]
fn get_member(id: u64) -> Result<Member, Error> {
    match _get_member(&id) {
        Some(member) => Ok(member),
        None => Err(Error::MemberNotFound),
    }
}

fn _get_member(id: &u64) -> Option<Member> {
    MEMBERS.with(|service| service.borrow().get(id))
}

// add a new member by the admin
#[ic_cdk::update]
fn add_member(mem: String) -> Result<Member, Error> {
    assert!(_is_caller_admin(), "You are not authorized to add members");

    let mem_principal = Principal::from_text(&mem);
    if mem_principal.is_err() {
        return Err(Error::InvalidInput {
            msg: format!("Mem is not a principal"),
        });
    }

    let id = ID_COUNT
        .with(|counter| {
            let current_value = *counter.borrow().get();
            counter.borrow_mut().set(current_value + 1)
        })
        .expect("cannot increment id counter for the members");
    let member = Member {
        id,
        principal_id: mem,
    };
    insert_member(&member);
    Ok(member)
}

fn _is_caller_admin() -> bool {
    let caller = caller().to_string();
    // assert(caller == ADMIN_PRINCIPAL_ID,"You are not authorized to add members");
    if caller == ADMIN_PRINCIPAL_ID {
        true
    } else {
        false
    }
}

fn insert_member(member: &Member) {
    MEMBERS.with(|service| service.borrow_mut().insert(member.id, member.clone()));
}

//update the member
#[ic_cdk::update]
fn update_member(id: u64, princ_id: String) -> Result<Member, Error> {
    assert!(
        _is_caller_admin(),
        "You are not authorized to update members"
    );
    let is_payload_principal = Principal::from_text(&princ_id);
    if is_payload_principal.is_err() {
        return Err(Error::InvalidInput {
            msg: format!("Mem is not a principal"),
        });
    }
    match MEMBERS.with(|service| service.borrow().get(&id)) {
        Some(mut member) => {
            member.principal_id = princ_id;
            insert_member(&member);
            Ok(member)
        }
        None => Err(Error::MemberNotFound),
    }
}

//delete a member from the list
#[ic_cdk::update]
fn delete_member(id: u64) -> Result<Member, Error> {
    assert!(
        _is_caller_admin(),
        "You are not authorized to delete members"
    );
    match MEMBERS.with(|service| service.borrow_mut().remove(&id)) {
        Some(member) => Ok(member),
        None => Err(Error::MemberNotFound),
    }
}

//get all members from the smart contract
#[ic_cdk::query]
fn get_all_members() -> Result<Vec<Member>, Error> {
    MEMBERS.with(|stores| Ok(stores.borrow().iter().map(|(_, s)| s.clone()).collect()))
}

//search for all the tasks to get matching ones
fn _search_member(_query: String) -> bool {
    let members = MEMBERS.with(|members| {
        let members = members
            .borrow()
            .iter()
            .filter(|(_, t)| (t.principal_id == _query))
            .map(|(_, v)| v.clone())
            .collect::<Vec<Member>>();
        return members;
    });
    if members.len() > 0 {
        return true;
    } else {
        return false;
    }
}

//get the task using the tak id
#[ic_cdk::query]
fn get_task(id: u64) -> Result<TASK, Error> {
    match _get_task(&id) {
        Some(message) => Ok(message),

        None => Err(Error::TaskNotFound),
    }
}

//get all tasks assigned to a specific user which are either completed or not
#[ic_cdk::query]
fn get_tasks_by_user(_user: Principal, completed: bool) -> Result<Vec<TASK>, Error> {
    let tasks = TASKS.with(|tasks| {
        let tasks = tasks
            .borrow()
            .iter()
            .filter(|(_, t)| (t.assigned_to == _user.to_string() && t.is_done == completed))
            .map(|(_, v)| v.clone())
            .collect::<Vec<TASK>>();
        Ok(tasks)
    })?;
    Ok(tasks)
}

//calculate and convert hours to nanoseconds
fn _hours_to_nanoseconds(hours: u64) -> u64 {
    let hour_to_seconds = hours * 60 * 60;
    let second_in_nanoseconds = 1000000000;
    return hour_to_seconds * second_in_nanoseconds;
}

//complete a task by the person assigned to it
#[ic_cdk::update]
fn complete_task(id: u64) -> Result<TASK, Error> {
    let caller = caller().to_string();
    match TASKS.with(|task| task.borrow().get(&id)) {
        Some(mut task) => {
            assert!((!task.is_done), "Task is already completed");
            assert!(
                task.assigned_to == caller,
                "You are not the assigne of this task"
            );
            let task_deadline = task.start_time + _hours_to_nanoseconds(task.deadline as u64);
            assert!(task_deadline < time(), "Deadline has already passed");
            task.is_done = true;
            do_insert(&task);
            Ok(task)
        }
        None => Err(Error::TaskNotFound),
    }
}

//search for all the tasks to get matching ones
#[ic_cdk::query]
fn search_tasks(_query: String) -> Result<Vec<TASK>, Error> {
    let tasks = TASKS.with(|tasks| {
        let tasks = tasks
            .borrow()
            .iter()
            .filter(|(_, t)| (t.title.contains(&_query) || t.description.contains(&_query)))
            .map(|(_, v)| v.clone())
            .collect::<Vec<TASK>>();
        Ok(tasks)
    })?;
    Ok(tasks)
}

// add a new task
#[ic_cdk::update]
fn add_task(task: TASKPayload) -> Result<TASK, Error> {
    let assigned_member: String = task.assigned_to.clone();
    assert!(_is_caller_admin(), "You are not authorized to add tasks");
    // Validates payload
    let check_payload = _check_input(&task);
    // Returns an error if validations failed
    if check_payload.is_err() {
        return Err(check_payload.err().unwrap());
    }
    assert!(
        _search_member(assigned_member),
        "Assigned member does not exist"
    );
    let id = ID_COUNTER
        .with(|counter| {
            let current_value = *counter.borrow().get();
            counter.borrow_mut().set(current_value + 1)
        })
        .expect("cannot increment id counter for the tasks");

    let message = TASK {
        id,
        title: task.title,
        description: task.description,
        assigned_to: task.assigned_to,
        is_done: false,
        start_time: time(),
        deadline: task.deadline,
        updated_at: None,
    };
    do_insert(&message);
    Ok(message)
}

//update the task
#[ic_cdk::update]
fn update_task(id: u64, payload: TASKPayload) -> Result<TASK, Error> {
    assert!(_is_caller_admin(), "You are not authorized to update tasks");
    // Validates payload
    let check_payload = _check_input(&payload);
    // Returns an error if validations failed
    if check_payload.is_err() {
        return Err(check_payload.err().unwrap());
    }
    match TASKS.with(|service| service.borrow().get(&id)) {
        Some(mut message) => {
            message.title = payload.title;
            message.description = payload.description;
            message.assigned_to = payload.assigned_to;
            message.is_done = false;
            message.start_time = time();
            message.deadline = payload.deadline;
            message.updated_at = Some(time());
            do_insert(&message);

            Ok(message)
        }
        None => Err(Error::TaskNotFound),
    }
}

// helper method to perform insert.
fn do_insert(message: &TASK) {
    TASKS.with(|service| service.borrow_mut().insert(message.id, message.clone()));
}

//delete a task
#[ic_cdk::update]
fn delete_task(id: u64) -> Result<TASK, Error> {
    assert!(_is_caller_admin(), "You are not authorized to add tasks");
    match TASKS.with(|service| service.borrow_mut().remove(&id)) {
        Some(message) => Ok(message),
        None => Err(Error::TaskNotFound),
    }
}

#[derive(candid::CandidType, Deserialize, Serialize)]
enum Error {
    InvalidInput { msg: String },
    TaskNotFound,
    MemberNotFound,
}

// a helper method to get a message by id. used in get_message/update_message
fn _get_task(id: &u64) -> Option<TASK> {
    TASKS.with(|service| service.borrow().get(id))
}

// Helper function to check the input data of the payload
fn _check_input(payload: &TASKPayload) -> Result<(), Error> {
    let check_payload = payload.validate();
    if check_payload.is_err() {
        return Err(Error::InvalidInput {
            msg: check_payload.err().unwrap().to_string(),
        });
    } else if Principal::from_text(&payload.assigned_to).is_err() {
        return Err(Error::InvalidInput {
            msg: format!(
                "Assigned_to = {} isn't a valid principal.",
                payload.assigned_to
            ),
        });
    } else {
        Ok(())
    }
}

// Function to get all tasks
#[ic_cdk::query]
fn get_all_tasks() -> Result<Vec<TASK>, Error> {
    TASKS.with(|tasks| Ok(tasks.borrow().iter().map(|(_, s)| s.clone()).collect()))
}

// need this to generate candid
ic_cdk::export_candid!();
