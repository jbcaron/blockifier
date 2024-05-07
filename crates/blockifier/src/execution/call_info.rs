use std::collections::{HashMap, HashSet};
use std::iter::Sum;
use std::ops::Add;

use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use parity_scale_codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use starknet_api::core::{ClassHash, ContractAddress, EthAddress, PatriciaKey};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::patricia_key;
use starknet_api::state::StorageKey;
use starknet_api::transaction::{EventContent, L2ToL1Payload};

use crate::execution::entry_point::CallEntryPoint;
use crate::fee::gas_usage::get_message_segment_length;
use crate::state::cached_state::StorageEntry;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Encode, Decode)]
#[cfg_attr(feature = "scale-info", derive(scale_info::TypeInfo))]
pub struct Retdata(pub Vec<StarkFelt>);

#[macro_export]
macro_rules! retdata {
    ( $( $x:expr ),* ) => {
        Retdata(vec![$($x),*])
    };
}

#[cfg_attr(test, derive(Clone))]
#[derive(Debug, Default, Eq, PartialEq, Serialize, Encode, Decode)]
pub struct OrderedEvent {
    #[codec(encoded_as = "crate::scale_codecs::USizeCodec")]
    pub order: usize,
    pub event: EventContent,
}

#[cfg(feature = "scale-info")]
impl scale_info::TypeInfo for OrderedEvent {
    type Identity = Self;

    fn type_info() -> scale_info::Type {
        scale_info::Type::builder()
            .path(scale_info::Path::new("OrderedEvent", module_path!()))
            .composite(
                scale_info::build::Fields::named()
                    .field(|f| f.ty::<u64>().name("order").type_name("u64"))
                    .field(|f| f.ty::<EventContent>().name("event").type_name("event_content")),
            )
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone)]
pub struct MessageL1CostInfo {
    pub l2_to_l1_payload_lengths: Vec<usize>,
    pub message_segment_length: usize,
}

impl MessageL1CostInfo {
    pub fn calculate<'a>(
        call_infos: impl Iterator<Item = &'a CallInfo>,
        l1_handler_payload_size: Option<usize>,
    ) -> Self {
        let mut l2_to_l1_payload_lengths = Vec::new();
        for call_info in call_infos {
            l2_to_l1_payload_lengths.extend(call_info.get_l2_to_l1_payload_lengths());
        }

        let message_segment_length =
            get_message_segment_length(&l2_to_l1_payload_lengths, l1_handler_payload_size);

        Self { l2_to_l1_payload_lengths, message_segment_length }
    }
}

#[cfg_attr(test, derive(Clone))]
#[derive(Debug, Default, Eq, PartialEq, Serialize, Encode, Decode)]
#[cfg_attr(feature = "scale-info", derive(scale_info::TypeInfo))]
pub struct MessageToL1 {
    pub to_address: EthAddress,
    pub payload: L2ToL1Payload,
}

#[cfg_attr(test, derive(Clone))]
#[derive(Debug, Default, Eq, PartialEq, Serialize, Encode, Decode)]
pub struct OrderedL2ToL1Message {
    #[codec(encoded_as = "crate::scale_codecs::USizeCodec")]
    pub order: usize,
    pub message: MessageToL1,
}

pub fn get_payload_lengths(l2_to_l1_messages: &[OrderedL2ToL1Message]) -> Vec<usize> {
    l2_to_l1_messages.iter().map(|message| message.message.payload.0.len()).collect()
}

#[cfg(feature = "scale-info")]
impl scale_info::TypeInfo for OrderedL2ToL1Message {
    type Identity = Self;

    fn type_info() -> scale_info::Type {
        scale_info::Type::builder()
            .path(scale_info::Path::new("OrderedL2ToL1Message", module_path!()))
            .composite(
                scale_info::build::Fields::named()
                    .field(|f| f.ty::<u64>().name("order").type_name("u64"))
                    .field(|f| f.ty::<MessageToL1>().name("message").type_name("MessageToL1")),
            )
    }
}

/// Represents the effects of executing a single entry point.
#[cfg_attr(test, derive(Clone))]
#[derive(Debug, Default, Eq, PartialEq, Serialize, Encode, Decode)]
#[cfg_attr(feature = "scale-info", derive(scale_info::TypeInfo))]
pub struct CallExecution {
    pub retdata: Retdata,
    pub events: Vec<OrderedEvent>,
    pub l2_to_l1_messages: Vec<OrderedL2ToL1Message>,
    pub failed: bool,
    pub gas_consumed: u64,
}

// This struct is used to implement `serde` functionality in a remote `ExecutionResources` Struct.
#[derive(Debug, Default, Deserialize, derive_more::From, Eq, PartialEq, Serialize)]
#[serde(remote = "ExecutionResources")]
struct ExecutionResourcesDef {
    n_steps: usize,
    n_memory_holes: usize,
    builtin_instance_counter: HashMap<String, usize>,
}

#[derive(Default)]
pub struct ExecutionSummary {
    pub executed_class_hashes: HashSet<ClassHash>,
    pub visited_storage_entries: HashSet<StorageEntry>,
    pub l2_to_l1_payload_lengths: Vec<usize>,
    pub n_events: usize,
}

impl Add for ExecutionSummary {
    type Output = Self;

    fn add(mut self, other: Self) -> Self {
        self.executed_class_hashes.extend(other.executed_class_hashes);
        self.visited_storage_entries.extend(other.visited_storage_entries);
        self.l2_to_l1_payload_lengths.extend(other.l2_to_l1_payload_lengths);
        self.n_events += other.n_events;
        self
    }
}

impl Sum for ExecutionSummary {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(ExecutionSummary::default(), |acc, x| acc + x)
    }
}

#[derive(Debug, Default)]
pub struct TestExecutionSummary {
    pub num_of_events: usize,
    pub num_of_messages: usize,
    pub class_hash: ClassHash,
    pub storage_address: ContractAddress,
    pub storage_key: StorageKey,
}

impl TestExecutionSummary {
    pub fn new(
        num_of_events: usize,
        num_of_messages: usize,
        class_hash: ClassHash,
        storage_address: &str,
        storage_key: &str,
    ) -> Self {
        TestExecutionSummary {
            num_of_events,
            num_of_messages,
            class_hash,
            storage_address: ContractAddress(patricia_key!(storage_address)),
            storage_key: StorageKey(patricia_key!(storage_key)),
        }
    }

    pub fn to_call_info(&self) -> CallInfo {
        CallInfo {
            call: CallEntryPoint {
                class_hash: Some(self.class_hash),
                storage_address: self.storage_address,
                ..Default::default()
            },
            execution: CallExecution {
                events: (0..self.num_of_events).map(|_| OrderedEvent::default()).collect(),
                l2_to_l1_messages: (0..self.num_of_messages)
                    .map(|i| OrderedL2ToL1Message {
                        order: i,
                        message: MessageToL1 {
                            to_address: EthAddress::default(),
                            payload: L2ToL1Payload(vec![StarkFelt::default()]),
                        },
                    })
                    .collect(),
                ..Default::default()
            },
            accessed_storage_keys: vec![self.storage_key].into_iter().collect(),
            ..Default::default()
        }
    }
}

/// Represents the full effects of executing an entry point, including the inner calls it invoked.
#[derive(Debug, Default, Eq, PartialEq, Serialize, Encode, Decode)]
pub struct CallInfo {
    pub call: CallEntryPoint,
    pub execution: CallExecution,
    #[serde(with = "ExecutionResourcesDef")]
    pub resources: ExecutionResources,
    pub inner_calls: Vec<CallInfo>,

    // Additional information gathered during execution.
    pub storage_read_values: Vec<StarkFelt>,

    #[codec(encoded_as = "crate::scale_codecs::HashSetCodec::<StorageKey>")]
    pub accessed_storage_keys: HashSet<StorageKey>,
}

#[cfg(feature = "scale-info")]
impl scale_info::TypeInfo for CallInfo {
    type Identity = Self;

    fn type_info() -> scale_info::Type {
        scale_info::Type::builder()
            .path(scale_info::Path::new("CallInfo", module_path!()))
            .composite(
                scale_info::build::Fields::named()
                    .field(|f| f.ty::<CallEntryPoint>().name("call").type_name("CallEntryPoint"))
                    .field(|f| f.ty::<CallExecution>().name("execution").type_name("CallExecution"))
                    .field(|f| {
                        f.ty::<ExecutionResources>()
                            .name("resources")
                            .type_name("ExecutionResources")
                    })
                    .field(|f| {
                        f.ty::<Vec<CallInfo>>().name("inner_calls").type_name("Vec<CallInfo>")
                    })
                    .field(|f| {
                        f.ty::<Vec<StarkFelt>>()
                            .name("storage_read_values")
                            .type_name("Vec<StarkFelt>")
                    })
                    .field(|f| {
                        f.ty::<Vec<StorageKey>>()
                            .name("accessed_storage_keys")
                            .type_name("Vec<StorageKey>")
                    }),
            )
    }
}

impl CallInfo {
    pub fn iter(&self) -> CallInfoIter<'_> {
        let call_infos = vec![self];
        CallInfoIter { call_infos }
    }

    pub fn get_l2_to_l1_payload_lengths(&self) -> Vec<usize> {
        self.iter().fold(Vec::new(), |mut acc, call_info| {
            acc.extend(get_payload_lengths(&call_info.execution.l2_to_l1_messages));
            acc
        })
    }

    pub fn summarize(&self) -> ExecutionSummary {
        let mut executed_class_hashes: HashSet<ClassHash> = HashSet::new();
        let mut visited_storage_entries: HashSet<StorageEntry> = HashSet::new();
        let mut n_events: usize = 0;
        let mut l2_to_l1_payload_lengths = Vec::new();

        for call_info in self.iter() {
            let class_hash =
                call_info.call.class_hash.expect("Class hash must be set after execution.");
            executed_class_hashes.insert(class_hash);

            let call_storage_entries = call_info
                .accessed_storage_keys
                .iter()
                .map(|storage_key| (call_info.call.storage_address, *storage_key));
            visited_storage_entries.extend(call_storage_entries);

            n_events += call_info.execution.events.len();

            l2_to_l1_payload_lengths
                .extend(get_payload_lengths(&call_info.execution.l2_to_l1_messages));
        }

        ExecutionSummary {
            executed_class_hashes,
            visited_storage_entries,
            l2_to_l1_payload_lengths,
            n_events,
        }
    }
}

pub struct CallInfoIter<'a> {
    call_infos: Vec<&'a CallInfo>,
}

impl<'a> Iterator for CallInfoIter<'a> {
    type Item = &'a CallInfo;

    fn next(&mut self) -> Option<Self::Item> {
        let call_info = self.call_infos.pop()?;

        // Push order is right to left.
        self.call_infos.extend(call_info.inner_calls.iter().rev());
        Some(call_info)
    }
}
