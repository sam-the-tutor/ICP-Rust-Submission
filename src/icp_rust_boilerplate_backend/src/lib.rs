#[macro_use]
extern crate serde;
use candid::{Decode, Encode,CandidType};
use ic_cdk::api::time;
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::{BoundedStorable, Cell, DefaultMemoryImpl, StableBTreeMap, Storable};
use std::{borrow::Cow, cell::RefCell};
// use validator::Validate;

use ic_cdk::api::caller;

// use candid::Principal;

type Memory = VirtualMemory<DefaultMemoryImpl>;
type IdCell = Cell<u64, Memory>;

const ADMIN_PRINCIPAL_ID: &str = "2vxsx-fae";
#[derive(CandidType, Clone, Serialize, Deserialize, Default)]
struct Task {
    id: u64,
    title: String,           //title of the task
    description: String,     //a brief description of the task
    assigned_to: String,     // the team member to whom it is assigned
    is_done: bool,           // to track whether the task has been completed
    start_time: u64,         //the time at which the task started
    deadline: u8,            //deadline in hours
    updated_at: Option<u64>, 
}



impl Storable for Task {
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
}

impl BoundedStorable for Task {
    const MAX_SIZE: u32 = 1024;
    const IS_FIXED_SIZE: bool = false;
}
#[derive(candid::CandidType, Clone, Serialize, Deserialize, Default)]
struct Member {
    id:u64,
    principal_id : String,
}


impl Storable for Member {
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
}

impl BoundedStorable for Member {
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

    static TASK_STORAGE: RefCell<StableBTreeMap<u64, Task, Memory>> =
        RefCell::new(StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(1)))
    ));

    static MEMBER_STORAGE: RefCell<StableBTreeMap<u64, Member, Memory>> =
        RefCell::new(StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(2)))
    ));

}

//payloads for the task and the member

#[derive(CandidType, Serialize, Deserialize, Default)]
struct MemberPayload {
    principal_id: String,
}


#[derive(CandidType, Serialize, Deserialize, Default)]
struct TaskPayload {
    title: String,
    description: String,
    assigned_to: String,
    deadline: u8,
}

//get all tasks
#[ic_cdk::query]
fn get_all_tasks() -> Result<Vec<Task>, Error> {
    let tasks_map: Vec<(u64, Task)> = TASK_STORAGE.with(|service| service.borrow().iter().collect());
    let tasks: Vec<Task> = tasks_map.into_iter().map(|(_, task)| task).collect();

    if !tasks.is_empty() {
        Ok(tasks)
    } else {
        Err(Error::NotFound {
            msg: "No tasks found.".to_string(),
        })
    }
}

//get one task using the id
#[ic_cdk::query]
fn get_task(id: u64) -> Result<Task, Error> {
    match _get_task(&id) {
        Some(task) => Ok(task),
        None => Err(Error::NotFound {
            msg: format!("Task with id={} not found.", id),
        }),
    }
}

fn _get_task(id: &u64) -> Option<Task> {
    TASK_STORAGE.with(|s| s.borrow().get(id))
}



//create a new task
#[ic_cdk::update]
fn create_task(payload: TaskPayload) -> Result<Task,Error> {

    if !_is_caller_admin() {
        return Err(Error::NotAuthorized)
    }else if !is_member(payload.assigned_to.clone()) {
        return Err(Error::MemberNotFound)
    }else{

    let id = ID_COUNTER
        .with(|counter| {
            let current_value = *counter.borrow().get();
            counter.borrow_mut().set(current_value + 1)
        })
        .expect("Cannot increment id counter");

    let task = Task {
    id,
    title: payload.title,           //title of the task
    description: payload.description,     //a brief description of the task
    assigned_to: payload.assigned_to,     // the team member to whom it is assigned
    is_done: false,           // to track whether the task has been completed
    start_time: time(),         //the time at which the task started
    deadline: payload.deadline,            //deadline in hours
    updated_at: None,
    };
    do_insert(&task);
    Ok(task)}
}

fn do_insert(task: &Task) {
    TASK_STORAGE.with(|service| service.borrow_mut().insert(task.id, task.clone()));
}

//update the task

#[ic_cdk::update]
fn update_task(id: u64, payload: TaskPayload) -> Result<Task, Error> {
    if !_is_caller_admin() {
     return Err(Error::NotAuthorized)
    }
    let task_option: Option<Task> = TASK_STORAGE.with(|service| service.borrow().get(&id));

    match task_option {
        
        Some(mut task) => {

            let task_deadline = task.start_time + _hours_to_nanoseconds(task.deadline as u64);
            if task_deadline < time(){
                return Err(Error::DeadlineAlreadyPassed)
            }else{
            task.title= payload.title;          //title of the task
            task.description= payload.description;     //a brief description of the task
            task.assigned_to= payload.assigned_to;     // the team member to whom it is assigned
            task.is_done= task.is_done;           // to track whether the task has been completed
            task.start_time= task.start_time;         //the time at which the task started
            task.deadline= payload.deadline;            //deadline in hours
            task.updated_at= Some(time());
            do_insert(&task);
            Ok(task)}
        }
        None => Err(Error::NotFound {
            msg: format!("Task with id={} not found.", id),
        }),
    }
}

#[ic_cdk::update]
fn delete_task(id: u64) -> Result<Task, Error> {
    assert!(
        _is_caller_admin(),
        "You are not authorized to delete a task"
    );
    match TASK_STORAGE.with(|service| service.borrow_mut().remove(&id)) {
        Some(task) => Ok(task),
        None => Err(Error::NotFound {
            msg: format!("Task with id={} not found.", id),
        }),
    }
}


fn _hours_to_nanoseconds(hours: u64) -> u64 {
    let hour_to_seconds = hours * 60 * 60;
    let second_in_nanoseconds = 1000000000;
    return hour_to_seconds * second_in_nanoseconds;
}





#[ic_cdk::update]
fn complete_task(id: u64) -> Result<Task, Error> {
    let caller = caller().to_string();
    match TASK_STORAGE.with(|task| task.borrow().get(&id)) {
        Some(mut task) => {
             let task_deadline = task.start_time + _hours_to_nanoseconds(task.deadline as u64);

             if task.is_done ==true {
                return Err(Error::TaskAlreadyDone)
             }else if task_deadline < time() {
               return  Err(Error::DeadlineAlreadyPassed)
             }else if task.assigned_to != caller {
                 return Err(Error::NotAuthorized)
                }else{
                 task.is_done = true;

                 do_insert(&task);
                 Ok(task)
             }

        }
        None => Err(Error::TaskNotFound),
    }
}

#[ic_cdk::query]
fn search_task(query: String) -> Result<Vec<Task>, Error> {
    let lower_case_query = query.to_lowercase();
    let filtered_tasks = TASK_STORAGE.with(|storage| {
        storage.borrow()
            .iter()
            .filter(|(_, task)| {
                task.title.to_lowercase().contains(&lower_case_query) || 
                task.description.to_lowercase().contains(&lower_case_query) 
            })
            .map(|(_, task)| task.clone())
            .collect::<Vec<Task>>()
    });

    if filtered_tasks.is_empty() {
        Err(Error::NotFound {
            msg: "No matching tasks found.".to_string(),
        })
    } else {
        Ok(filtered_tasks)
    }
}


//get tasks by user
#[ic_cdk::query]
fn get_tasks_by_member(member_principal: String) -> Result<Vec<Task>, Error> {
    let lower_case_query = member_principal.to_lowercase();
    let filtered_tasks = TASK_STORAGE.with(|storage| {
        storage.borrow()
            .iter()
            .filter(|(_, task)| {
                task.assigned_to.to_lowercase().contains(&lower_case_query) 
            })
            .map(|(_, task)| task.clone())
            .collect::<Vec<Task>>()
    });

    if filtered_tasks.is_empty() {
        Err(Error::NotFound {
            msg: "No matching tasks for the member specified.".to_string(),
        })
    } else {
        Ok(filtered_tasks)
    }
}


#[ic_cdk::query]
fn get_member(id: u64) -> Result<Member, Error> {
    match _get_member(&id) {
        Some(expense) => Ok(expense),
        None => Err(Error::NotFound {
            msg: format!("Member with id={} not found.", id),
        }),
    }
}

fn _get_member(id: &u64) -> Option<Member> {
    MEMBER_STORAGE.with(|s| s.borrow().get(id))
}


#[ic_cdk::update]
fn add_member(payload: MemberPayload) -> Result<Member,Error> {
    if !_is_caller_admin(){
        return Err(Error::NotAuthorized)
    }
    let id = ID_COUNTER
        .with(|counter| {
            let current_value = *counter.borrow().get();
            counter.borrow_mut().set(current_value + 1)
        })
        .expect("Cannot increment id counter");

    let member = Member {
        id,
        principal_id : payload.principal_id,
    };
    do_insert_member(&member);
    Ok(member)
}


fn do_insert_member(member: &Member) {
    MEMBER_STORAGE.with(|service| service.borrow_mut().insert(member.id, member.clone()));
}


#[ic_cdk::update]
fn update_member(id: u64, payload: MemberPayload) -> Result<Member, Error> {
    if !_is_caller_admin(){
        return Err(Error::NotAuthorized)
    }
    let member_option: Option<Member> = MEMBER_STORAGE.with(|service| service.borrow().get(&id));

    match member_option {
        Some(mut member) => {
            member.principal_id = payload.principal_id;
            do_insert_member(&member);
            Ok(member)
        }
        None => Err(Error::NotFound {
            msg: format!("Member with id={} not found.", id),
        }),
    }
}

#[ic_cdk::update]
fn delete_member(id: u64) -> Result<Member, Error> {
    if !_is_caller_admin(){
        return Err(Error::NotAuthorized)
    }
    match MEMBER_STORAGE.with(|service| service.borrow_mut().remove(&id)) {
        Some(member) => Ok(member),
        None => Err(Error::NotFound {
            msg: format!("Member with id={} not found.", id),
        }),
    }
}




#[ic_cdk::query]
fn is_member(query: String) -> bool {
    let filtered_tasks = MEMBER_STORAGE.with(|storage| {
        storage.borrow()
            .iter()
            .filter(|(_, member)| {
                member.principal_id == query 
            })
            .map(|(_, member)| member.clone())
            .collect::<Vec<Member>>()
    });

    if filtered_tasks.is_empty() {
        return false
    } else {return true    }
}

#[ic_cdk::query]
fn get_all_members() -> Result<Vec<Member>, Error> {
    let members_map: Vec<(u64, Member)> = MEMBER_STORAGE.with(|service| service.borrow().iter().collect());
    let members: Vec<Member> = members_map.into_iter().map(|(_, member)| member).collect();

    if !members.is_empty() {
        Ok(members)
    } else {
        Err(Error::NotFound {
            msg: "No members found.".to_string(),
        })
    }
}

fn _is_caller_admin() -> bool {
    let caller = caller().to_string();
    if caller == ADMIN_PRINCIPAL_ID {
        true
    } else {
        false
    }
}


#[derive(CandidType, Deserialize, Serialize)]
enum Error {
    InvalidInput { msg: String },
    NotFound { msg: String },
    TaskNotFound,
    TaskAlreadyDone,
    DeadlineAlreadyPassed,
    NotAuthorized,
    MemberNotFound
}

ic_cdk::export_candid!();

