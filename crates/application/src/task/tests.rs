use crate::{
    ApplicationError, Clock, CreateTaskHandler, DeleteTaskHandler, GetTaskHandler,
    ListTasksHandler, TaskIdGenerator, TaskRepository, TaskRepositoryError, UpdateTaskHandler,
};
use ora_contracts::{
    CreateTaskRequest, CreateTaskResponse, DeleteTaskRequest, DeleteTaskResponse, GetTaskRequest,
    GetTaskResponse, ListTasksRequest, ListTasksResponse, Task as ContractTask,
    TaskStatus as ContractTaskStatus, UpdateTaskRequest, UpdateTaskResponse,
};
use ora_domain::{
    AuditFields, ProjectId, Task, TaskId, TaskStatus as DomainTaskStatus, WorktreeId,
};
use ora_logging::{with_recorded_trace_logging, with_trace_logging};
use pretty_assertions::assert_eq;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

/// Verifies create handlers build domain tasks and return the shared contract response.
#[test]
fn creates_tasks_with_generated_identity_and_clock_values() {
    with_trace_logging(|| {
        let repository = Rc::new(FakeTaskRepository::default());
        let handler = CreateTaskHandler::new(
            repository.clone(),
            FixedTaskIdGenerator::new("task-1"),
            FixedClock::new(1_700_000_000_000),
        );

        let response = match handler.handle(CreateTaskRequest {
            project_id: "project-1".to_string(),
            title: "Ship handlers".to_string(),
            status: ContractTaskStatus::Doing,
            worktree_id: Some("worktree-1".to_string()),
        }) {
            Ok(response) => response,
            Err(error) => panic!("create handler failed: {error}"),
        };

        assert_eq!(
            response,
            CreateTaskResponse {
                task: ContractTask {
                    id: "task-1".to_string(),
                    project_id: "project-1".to_string(),
                    title: "Ship handlers".to_string(),
                    status: ContractTaskStatus::Doing,
                    worktree_id: Some("worktree-1".to_string()),
                },
            }
        );
        assert_eq!(
            repository.visible_tasks(),
            vec![Task::new(
                TaskId::new("task-1"),
                ProjectId::new("project-1"),
                "Ship handlers",
                DomainTaskStatus::Doing,
                Some(WorktreeId::new("worktree-1")),
                AuditFields::new(1_700_000_000_000, 1_700_000_000_000, false),
            )]
        );
    });
}

/// Verifies get handlers return the shared contract projection for existing tasks.
#[test]
fn gets_tasks_by_identifier() {
    with_trace_logging(|| {
        let repository = Rc::new(FakeTaskRepository::with_tasks(vec![Task::new(
            TaskId::new("task-1"),
            ProjectId::new("project-1"),
            "Ship handlers",
            DomainTaskStatus::Todo,
            None,
            AuditFields::new(1, 2, false),
        )]));
        let handler = GetTaskHandler::new(repository);

        let response = match handler.handle(GetTaskRequest {
            task_id: "task-1".to_string(),
        }) {
            Ok(response) => response,
            Err(error) => panic!("get handler failed: {error}"),
        };

        assert_eq!(
            response,
            GetTaskResponse {
                task: ContractTask {
                    id: "task-1".to_string(),
                    project_id: "project-1".to_string(),
                    title: "Ship handlers".to_string(),
                    status: ContractTaskStatus::Todo,
                    worktree_id: None,
                },
            }
        );
    });
}

/// Verifies list handlers map every stored task into the shared contract payload.
#[test]
fn lists_visible_tasks() {
    with_trace_logging(|| {
        let repository = Rc::new(FakeTaskRepository::with_tasks(vec![
            Task::new(
                TaskId::new("task-1"),
                ProjectId::new("project-1"),
                "Ship handlers",
                DomainTaskStatus::Todo,
                None,
                AuditFields::new(1, 2, false),
            ),
            Task::new(
                TaskId::new("task-2"),
                ProjectId::new("project-2"),
                "Wire exports",
                DomainTaskStatus::Done,
                Some(WorktreeId::new("worktree-2")),
                AuditFields::new(3, 4, false),
            ),
        ]));
        let handler = ListTasksHandler::new(repository);

        let response = match handler.handle(ListTasksRequest {}) {
            Ok(response) => response,
            Err(error) => panic!("list handler failed: {error}"),
        };

        assert_eq!(
            response,
            ListTasksResponse {
                tasks: vec![
                    ContractTask {
                        id: "task-1".to_string(),
                        project_id: "project-1".to_string(),
                        title: "Ship handlers".to_string(),
                        status: ContractTaskStatus::Todo,
                        worktree_id: None,
                    },
                    ContractTask {
                        id: "task-2".to_string(),
                        project_id: "project-2".to_string(),
                        title: "Wire exports".to_string(),
                        status: ContractTaskStatus::Done,
                        worktree_id: Some("worktree-2".to_string()),
                    },
                ],
            }
        );
    });
}

/// Verifies update handlers preserve created timestamps while refreshing mutable fields.
#[test]
fn updates_tasks_with_refreshed_timestamps() {
    with_trace_logging(|| {
        let repository = Rc::new(FakeTaskRepository::with_tasks(vec![Task::new(
            TaskId::new("task-1"),
            ProjectId::new("project-1"),
            "Ship handlers",
            DomainTaskStatus::Todo,
            None,
            AuditFields::new(10, 20, false),
        )]));
        let handler = UpdateTaskHandler::new(repository.clone(), FixedClock::new(30));

        let response = match handler.handle(UpdateTaskRequest {
            task_id: "task-1".to_string(),
            project_id: "project-2".to_string(),
            title: "Ship updated handlers".to_string(),
            status: ContractTaskStatus::Done,
            worktree_id: Some("worktree-2".to_string()),
        }) {
            Ok(response) => response,
            Err(error) => panic!("update handler failed: {error}"),
        };

        assert_eq!(
            response,
            UpdateTaskResponse {
                task: ContractTask {
                    id: "task-1".to_string(),
                    project_id: "project-2".to_string(),
                    title: "Ship updated handlers".to_string(),
                    status: ContractTaskStatus::Done,
                    worktree_id: Some("worktree-2".to_string()),
                },
            }
        );
        assert_eq!(
            repository.visible_tasks(),
            vec![Task::new(
                TaskId::new("task-1"),
                ProjectId::new("project-2"),
                "Ship updated handlers",
                DomainTaskStatus::Done,
                Some(WorktreeId::new("worktree-2")),
                AuditFields::new(10, 30, false),
            )]
        );
    });
}

/// Verifies delete handlers keep the external CRUD contract while soft-deleting storage state.
#[test]
fn deletes_tasks_through_soft_delete_repository_calls() {
    with_trace_logging(|| {
        let repository = Rc::new(FakeTaskRepository::with_tasks(vec![Task::new(
            TaskId::new("task-1"),
            ProjectId::new("project-1"),
            "Ship handlers",
            DomainTaskStatus::Todo,
            None,
            AuditFields::new(10, 20, false),
        )]));
        let handler = DeleteTaskHandler::new(repository.clone(), FixedClock::new(40));

        let response = match handler.handle(DeleteTaskRequest {
            task_id: "task-1".to_string(),
        }) {
            Ok(response) => response,
            Err(error) => panic!("delete handler failed: {error}"),
        };

        assert_eq!(
            response,
            DeleteTaskResponse {
                task_id: "task-1".to_string(),
            }
        );
        assert_eq!(repository.visible_tasks(), Vec::<Task>::new());
        assert_eq!(
            repository.all_tasks(),
            vec![Task::new(
                TaskId::new("task-1"),
                ProjectId::new("project-1"),
                "Ship handlers",
                DomainTaskStatus::Todo,
                None,
                AuditFields::new(10, 40, true),
            )]
        );
    });
}

/// Verifies handlers expose stable application errors for missing tasks and repository failures.
#[test]
fn reports_application_errors() {
    with_trace_logging(|| {
        let missing_repository = Rc::new(FakeTaskRepository::default());
        let get_handler = GetTaskHandler::new(missing_repository);
        let failing_repository = Rc::new(FakeTaskRepository::default());
        failing_repository.fail_next(TaskRepositoryError::OperationFailed(
            "storage unavailable".to_string(),
        ));
        let list_handler = ListTasksHandler::new(failing_repository);

        let missing_error = match get_handler.handle(GetTaskRequest {
            task_id: "missing".to_string(),
        }) {
            Ok(response) => panic!("expected missing error, got response: {response:?}"),
            Err(error) => error,
        };
        let repository_error = match list_handler.handle(ListTasksRequest {}) {
            Ok(response) => panic!("expected repository error, got response: {response:?}"),
            Err(error) => error,
        };

        assert_eq!(
            missing_error,
            ApplicationError::TaskNotFound {
                task_id: "missing".to_string(),
            }
        );
        assert_eq!(
            repository_error,
            ApplicationError::TaskRepository {
                message: "storage unavailable".to_string(),
            }
        );
    });
}

/// Verifies task handlers emit structured success and failure events under a scoped subscriber.
#[test]
fn emits_structured_operational_events() {
    let recorder = EventRecorder::default();
    with_recorded_trace_logging(recorder.layer(), || {
        let create_repository = Rc::new(FakeTaskRepository::default());
        let create_handler = CreateTaskHandler::new(
            create_repository,
            FixedTaskIdGenerator::new("task-42"),
            FixedClock::new(5),
        );
        let get_handler = GetTaskHandler::new(Rc::new(FakeTaskRepository::default()));

        create_handler
            .handle(CreateTaskRequest {
                project_id: "project-1".to_string(),
                title: "Ship handlers".to_string(),
                status: ContractTaskStatus::Todo,
                worktree_id: None,
            })
            .unwrap();
        assert_eq!(
            get_handler
                .handle(GetTaskRequest {
                    task_id: "missing".to_string(),
                })
                .unwrap_err(),
            ApplicationError::TaskNotFound {
                task_id: "missing".to_string(),
            }
        );
    });

    assert_eq!(
        recorder.events(),
        vec![
            LoggedEvent {
                level: "INFO".to_string(),
                target: "ora_application::task::handlers".to_string(),
                fields: BTreeMap::from([
                    (
                        "message".to_string(),
                        "task operation completed".to_string()
                    ),
                    ("method".to_string(), "log_task_success".to_string()),
                    ("operation".to_string(), "create_task".to_string()),
                    ("task_id".to_string(), "task-42".to_string()),
                ]),
            },
            LoggedEvent {
                level: "ERROR".to_string(),
                target: "ora_application::task::handlers".to_string(),
                fields: BTreeMap::from([
                    ("error.kind".to_string(), "task_not_found".to_string()),
                    (
                        "error.message".to_string(),
                        "task not found: missing".to_string(),
                    ),
                    ("message".to_string(), "task operation failed".to_string()),
                    ("method".to_string(), "log_task_failure".to_string()),
                    ("operation".to_string(), "get_task".to_string()),
                    ("task_id".to_string(), "missing".to_string()),
                ]),
            },
        ]
    );
}

#[derive(Debug, Default)]
struct FakeTaskRepository {
    tasks: RefCell<Vec<Task>>,
    next_error: RefCell<Option<TaskRepositoryError>>,
}

impl FakeTaskRepository {
    /// Builds a fake repository seeded with the provided task rows.
    fn with_tasks(tasks: Vec<Task>) -> Self {
        Self {
            tasks: RefCell::new(tasks),
            next_error: RefCell::new(None),
        }
    }

    /// Configures the next repository call to fail with a deterministic error.
    fn fail_next(&self, error: TaskRepositoryError) {
        self.next_error.replace(Some(error));
    }

    /// Returns every non-deleted task so tests can assert visible repository state.
    fn visible_tasks(&self) -> Vec<Task> {
        self.tasks
            .borrow()
            .iter()
            .filter(|task| !task.audit_fields.is_deleted)
            .cloned()
            .collect()
    }

    /// Returns all stored tasks, including soft-deleted rows, for state assertions.
    fn all_tasks(&self) -> Vec<Task> {
        self.tasks.borrow().clone()
    }

    /// Returns a queued error when a test wants to simulate repository failure.
    fn take_error(&self) -> Result<(), TaskRepositoryError> {
        match self.next_error.borrow_mut().take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl TaskRepository for Rc<FakeTaskRepository> {
    fn create_task(&self, task: Task) -> Result<Task, TaskRepositoryError> {
        self.take_error()?;

        self.tasks.borrow_mut().push(task.clone());
        Ok(task)
    }

    fn find_task(&self, task_id: &TaskId) -> Result<Option<Task>, TaskRepositoryError> {
        self.take_error()?;

        Ok(self
            .tasks
            .borrow()
            .iter()
            .find(|task| task.id == *task_id && !task.audit_fields.is_deleted)
            .cloned())
    }

    fn list_tasks(&self) -> Result<Vec<Task>, TaskRepositoryError> {
        self.take_error()?;

        Ok(self.visible_tasks())
    }

    fn update_task(&self, task: Task) -> Result<Task, TaskRepositoryError> {
        self.take_error()?;

        let mut tasks = self.tasks.borrow_mut();
        if let Some(existing_task) = tasks.iter_mut().find(|existing_task| {
            existing_task.id == task.id && !existing_task.audit_fields.is_deleted
        }) {
            *existing_task = task.clone();
            Ok(task)
        } else {
            Err(TaskRepositoryError::OperationFailed(format!(
                "missing task during update: {}",
                task.id
            )))
        }
    }

    fn soft_delete_task(
        &self,
        task_id: &TaskId,
        deleted_at: i64,
    ) -> Result<bool, TaskRepositoryError> {
        self.take_error()?;

        let mut tasks = self.tasks.borrow_mut();
        if let Some(task) = tasks
            .iter_mut()
            .find(|task| task.id == *task_id && !task.audit_fields.is_deleted)
        {
            task.audit_fields.updated_at = deleted_at;
            task.audit_fields.is_deleted = true;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

struct FixedTaskIdGenerator {
    task_id: TaskId,
}

impl FixedTaskIdGenerator {
    /// Builds an identifier generator that always returns the provided task id.
    fn new(task_id: impl Into<String>) -> Self {
        Self {
            task_id: TaskId::new(task_id),
        }
    }
}

impl TaskIdGenerator for FixedTaskIdGenerator {
    fn generate_task_id(&self) -> TaskId {
        self.task_id.clone()
    }
}

struct FixedClock {
    timestamp_millis: i64,
}

impl FixedClock {
    /// Builds a clock that always returns the provided timestamp.
    fn new(timestamp_millis: i64) -> Self {
        Self { timestamp_millis }
    }
}

impl Clock for FixedClock {
    fn now_timestamp_millis(&self) -> i64 {
        self.timestamp_millis
    }
}

/// Captures one emitted event in a comparison-friendly structure for logging assertions.
#[derive(Clone, Debug, Eq, PartialEq)]
struct LoggedEvent {
    level: String,
    target: String,
    fields: BTreeMap<String, String>,
}

/// Records tracing events into shared memory so tests can assert full structured outcomes.
#[derive(Clone, Debug, Default)]
struct EventRecorder {
    events: Arc<Mutex<Vec<LoggedEvent>>>,
}

impl EventRecorder {
    /// Builds the recording layer attached to one scoped test subscriber.
    fn layer(&self) -> RecordingLayer {
        RecordingLayer {
            events: self.events.clone(),
        }
    }

    /// Returns every captured event in emission order.
    fn events(&self) -> Vec<LoggedEvent> {
        self.events.lock().unwrap().clone()
    }
}

/// Pushes each tracing event into the shared recorder without relying on global subscriber state.
#[derive(Clone, Debug)]
struct RecordingLayer {
    events: Arc<Mutex<Vec<LoggedEvent>>>,
}

impl<S> Layer<S> for RecordingLayer
where
    S: tracing::Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    /// Converts each event into a stable, fully comparable structure for test assertions.
    fn on_event(&self, event: &tracing::Event<'_>, _context: Context<'_, S>) {
        let mut visitor = EventFieldVisitor::default();
        event.record(&mut visitor);
        self.events.lock().unwrap().push(LoggedEvent {
            level: event.metadata().level().to_string(),
            target: event.metadata().target().to_string(),
            fields: visitor.fields,
        });
    }
}

/// Records tracing fields as strings because these tests care about semantic content, not JSON formatting.
#[derive(Debug, Default)]
struct EventFieldVisitor {
    fields: BTreeMap<String, String>,
}

impl tracing::field::Visit for EventFieldVisitor {
    /// Preserves string fields exactly as handler logs emitted them.
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    /// Preserves signed integers in decimal form for stable assertions.
    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    /// Preserves unsigned integers in decimal form for stable assertions.
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    /// Falls back to debug formatting for field types without a more specific visitor hook.
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.fields.insert(
            field.name().to_string(),
            format!("{value:?}").trim_matches('"').to_string(),
        );
    }
}
