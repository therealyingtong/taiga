use crate::circuit::{
    vp_circuit::{VPVerifyingInfo, ValidityPredicateVerifyingInfo, VampIRValidityPredicateCircuit},
    vp_examples::TrivialValidityPredicateCircuit,
};
use crate::shielded_ptx::NoteVPVerifyingInfoSet;
use borsh::{BorshDeserialize, BorshSerialize};
#[cfg(feature = "serde")]
use serde;
use std::path::PathBuf;

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ValidityPredicateRepresentation {
    // vampir has a unified circuit representation.
    VampIR(Vec<u8>),
    // Native halo2 circuits don't have a unified representatioin, enumerate the vp circuit examples for the moment.
    // TODO: figure out if we can have a unified circuit presentation. In theory, it's possible to separate the circuit system and proving system.
    Trivial,
    // TODO: add other vp types here if needed
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ValidityPredicateByteCode {
    circuit: ValidityPredicateRepresentation,
    inputs: Vec<u8>,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ApplicationByteCode {
    app_vp_bytecode: ValidityPredicateByteCode,
    dynamic_vp_bytecode: Vec<ValidityPredicateByteCode>,
}

impl ValidityPredicateByteCode {
    pub fn new(circuit: ValidityPredicateRepresentation, inputs: Vec<u8>) -> Self {
        Self { circuit, inputs }
    }

    pub fn generate_proof(self) -> VPVerifyingInfo {
        match self.circuit {
            ValidityPredicateRepresentation::VampIR(circuit) => {
                // TDDO: use the file_name api atm,
                // request vamp_ir to provide a api to generate circuit from bytes.
                let vamp_ir_circuit_file =
                    PathBuf::from(String::from_utf8_lossy(&circuit).to_string());
                let inputs_file = PathBuf::from(String::from_utf8_lossy(&self.inputs).to_string());
                let vp_circuit = VampIRValidityPredicateCircuit::from_vamp_ir_file(
                    &vamp_ir_circuit_file,
                    &inputs_file,
                );
                vp_circuit.get_verifying_info()
            }
            ValidityPredicateRepresentation::Trivial => {
                let vp = TrivialValidityPredicateCircuit::from_bytes(self.inputs);
                vp.get_verifying_info()
            }
        }
    }
}

impl ApplicationByteCode {
    pub fn new(
        app_vp_bytecode: ValidityPredicateByteCode,
        dynamic_vp_bytecode: Vec<ValidityPredicateByteCode>,
    ) -> Self {
        Self {
            app_vp_bytecode,
            dynamic_vp_bytecode,
        }
    }

    pub fn generate_proofs(self) -> NoteVPVerifyingInfoSet {
        let app_vp_verifying_info = self.app_vp_bytecode.generate_proof();

        let app_dynamic_vp_verifying_info = self
            .dynamic_vp_bytecode
            .into_iter()
            .map(|bytecode| bytecode.generate_proof())
            .collect();
        NoteVPVerifyingInfoSet::new(app_vp_verifying_info, app_dynamic_vp_verifying_info)
    }
}
