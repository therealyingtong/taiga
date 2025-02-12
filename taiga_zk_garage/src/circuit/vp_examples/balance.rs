use crate::circuit::circuit_parameters::CircuitParameters;
use crate::circuit::gadgets::field_addition::field_addition_gadget;
use crate::circuit::integrity::{
    ValidityPredicateInputNoteVariables, ValidityPredicateOutputNoteVariables,
};
use crate::circuit::validity_predicate::{ValidityPredicate, NUM_NOTE};
use crate::note::Note;
use plonk_core::{circuit::Circuit, constraint_system::StandardComposer, prelude::Error};

// BalanceValidityPredicate have a custom constraint with a + b = c,
// in which a, b are private inputs and c is a public input.
pub struct BalanceValidityPredicate<CP: CircuitParameters> {
    // basic "private" inputs to the VP
    input_notes: [Note<CP>; NUM_NOTE],
    output_notes: [Note<CP>; NUM_NOTE],
}

impl<CP: CircuitParameters> BalanceValidityPredicate<CP> {
    pub fn new(input_notes: [Note<CP>; NUM_NOTE], output_notes: [Note<CP>; NUM_NOTE]) -> Self {
        Self {
            input_notes,
            output_notes,
        }
    }
}

impl<CP> ValidityPredicate<CP> for BalanceValidityPredicate<CP>
where
    CP: CircuitParameters,
{
    fn get_input_notes(&self) -> &[Note<CP>; NUM_NOTE] {
        &self.input_notes
    }

    fn get_output_notes(&self) -> &[Note<CP>; NUM_NOTE] {
        &self.output_notes
    }

    fn custom_constraints(
        &self,
        composer: &mut StandardComposer<CP::CurveScalarField, CP::InnerCurve>,
        input_note_variables: &[ValidityPredicateInputNoteVariables],
        output_note_variables: &[ValidityPredicateOutputNoteVariables],
    ) -> Result<(), Error> {
        // check that all notes use the same app
        let var_app = input_note_variables[0].app_addr;
        for note_var in input_note_variables {
            composer.assert_equal(note_var.app_addr, var_app);
        }
        for note_var in output_note_variables {
            composer.assert_equal(note_var.app_addr, var_app);
        }

        // sum of the input note values
        let mut balance_input_var = composer.zero_var();
        for note_var in input_note_variables {
            balance_input_var =
                field_addition_gadget::<CP>(composer, balance_input_var, note_var.value);
        }
        // sum of the output note values
        let mut balance_output_var = composer.zero_var();
        for note_var in output_note_variables {
            balance_output_var =
                field_addition_gadget::<CP>(composer, balance_output_var, note_var.value);
        }
        composer.assert_equal(balance_input_var, balance_output_var);
        Ok(())
    }
}

impl<CP> Circuit<CP::CurveScalarField, CP::InnerCurve> for BalanceValidityPredicate<CP>
where
    CP: CircuitParameters,
{
    const CIRCUIT_ID: [u8; 32] = [0x00; 32];

    // Default implementation
    fn gadget(
        &mut self,
        composer: &mut StandardComposer<CP::CurveScalarField, CP::InnerCurve>,
    ) -> Result<(), Error> {
        self.gadget_vp(composer)
    }

    fn padded_circuit_size(&self) -> usize {
        1 << 17
    }
}

#[ignore]
#[test]
fn test_balance_vp_example() {
    use crate::app::App;
    use crate::circuit::circuit_parameters::PairingCircuitParameters as CP;
    use plonk_core::circuit::{verify_proof, VerifierData};
    type Fr = <CP as CircuitParameters>::CurveScalarField;
    type P = <CP as CircuitParameters>::InnerCurve;
    type PC = <CP as CircuitParameters>::CurvePC;
    use ark_std::test_rng;

    let mut rng = test_rng();
    let xan = App::<CP>::dummy(&mut rng);
    // input notes
    let input_notes = [(); NUM_NOTE].map(|_| Note::<CP>::dummy_from_app(xan.clone(), &mut rng));
    // output notes
    let mut output_notes = input_notes.clone();
    let tmp = output_notes[0].value;
    output_notes[0].value = output_notes[1].value;
    output_notes[1].value = tmp;

    let mut balance_vp = BalanceValidityPredicate {
        input_notes,
        output_notes,
    };

    let mut composer = StandardComposer::<Fr, P>::new();
    balance_vp.gadget(&mut composer).unwrap();
    composer.check_circuit_satisfied();
    println!("circuit size of balance_vp: {}", composer.circuit_bound());

    // Generate CRS
    let pp = CP::get_pc_setup_params(balance_vp.padded_circuit_size());

    // Compile the circuit
    let (pk, vk) = balance_vp.compile::<PC>(pp).unwrap();

    // Prover
    let (proof, public_input) = balance_vp.gen_proof::<PC>(pp, pk, b"Test").unwrap();

    // Verifier
    let verifier_data = VerifierData::new(vk, public_input);
    verify_proof::<Fr, P, PC>(pp, verifier_data.key, &proof, &verifier_data.pi, b"Test").unwrap();
}
