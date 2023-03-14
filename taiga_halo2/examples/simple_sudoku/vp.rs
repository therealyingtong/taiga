use halo2_proofs::{
    circuit::{floor_planner, Layouter},
    plonk::{self, keygen_pk, keygen_vk, Circuit, ConstraintSystem, Error},
};
use pasta_curves::pallas;

extern crate taiga_halo2;
use taiga_halo2::{
    circuit::{
        integrity::{OutputNoteVar, SpendNoteVar},
        note_circuit::NoteConfig,
        vp_circuit::{
            VPVerifyingInfo, ValidityPredicateCircuit, ValidityPredicateConfig,
            ValidityPredicateInfo,
        },
    },
    constant::{NUM_NOTE, SETUP_PARAMS_MAP},
    note::Note,
    proof::Proof,
    vp_circuit_impl,
    vp_vk::ValidityPredicateVerifyingKey,
};

use crate::circuit::{SudokuCircuit, SudokuConfig};
use rand::rngs::OsRng;

#[derive(Clone, Debug)]
pub struct SudokuVPConfig {
    note_config: NoteConfig,
    sudoku_config: SudokuConfig,
}

impl ValidityPredicateConfig for SudokuVPConfig {
    fn get_note_config(&self) -> NoteConfig {
        self.note_config.clone()
    }

    fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self {
        let note_config = Self::configure_note(meta);
        let sudoku_config = SudokuCircuit::configure(meta);
        Self {
            note_config,
            sudoku_config,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SudokuVP {
    pub sudoku: SudokuCircuit,
    spend_notes: [Note; NUM_NOTE],
    output_notes: [Note; NUM_NOTE],
}

impl ValidityPredicateCircuit for SudokuVP {
    type VPConfig = SudokuVPConfig;

    fn custom_constraints(
        &self,
        config: Self::VPConfig,
        layouter: impl Layouter<pallas::Base>,
        _spend_note_variables: &[SpendNoteVar],
        _output_note_variables: &[OutputNoteVar],
    ) -> Result<(), plonk::Error> {
        self.sudoku.synthesize(config.sudoku_config, layouter)
    }
}

impl ValidityPredicateInfo for SudokuVP {
    fn get_spend_notes(&self) -> &[Note; NUM_NOTE] {
        &self.spend_notes
    }

    fn get_output_notes(&self) -> &[Note; NUM_NOTE] {
        &self.output_notes
    }

    fn get_instances(&self) -> Vec<pallas::Base> {
        self.get_note_instances()
    }

    fn get_verifying_info(&self) -> VPVerifyingInfo {
        let mut rng = OsRng;
        let params = SETUP_PARAMS_MAP.get(&12).unwrap();
        let vk = keygen_vk(params, self).expect("keygen_vk should not fail");
        let pk = keygen_pk(params, vk.clone(), self).expect("keygen_pk should not fail");
        let instance = self.get_instances();
        let proof = Proof::create(&pk, params, self.clone(), &[&instance], &mut rng).unwrap();
        VPVerifyingInfo {
            vk,
            proof,
            instance,
        }
    }

    fn get_vp_description(&self) -> ValidityPredicateVerifyingKey {
        let params = SETUP_PARAMS_MAP.get(&12).unwrap();
        let vk = keygen_vk(params, self).expect("keygen_vk should not fail");
        ValidityPredicateVerifyingKey::from_vk(vk)
    }
}

impl SudokuVP {
    pub fn new(
        sudoku: SudokuCircuit,
        spend_notes: [Note; NUM_NOTE],
        output_notes: [Note; NUM_NOTE],
    ) -> Self {
        Self {
            sudoku,
            spend_notes,
            output_notes,
        }
    }
}

vp_circuit_impl!(SudokuVP);

#[cfg(test)]
mod tests {
    use taiga_halo2::{
        constant::NUM_NOTE,
        note::Note,
        nullifier::{Nullifier, NullifierKeyCom},
        vp_vk::ValidityPredicateVerifyingKey,
    };

    use ff::Field;
    use pasta_curves::pallas;
    use rand::rngs::OsRng;

    use halo2_proofs::{plonk, poly::commitment::Params};

    use crate::{circuit::SudokuCircuit, vp::SudokuVP};

    #[test]
    fn test_vp() {
        let mut rng = OsRng;
        let input_notes = [(); NUM_NOTE].map(|_| Note::dummy(&mut rng));
        let output_notes = [(); NUM_NOTE].map(|_| Note::dummy(&mut rng));

        const K: u32 = 13;
        let sudoku = SudokuCircuit {
            sudoku: [
                [7, 6, 9, 5, 3, 8, 1, 2, 4],
                [2, 4, 3, 7, 1, 9, 6, 5, 8],
                [8, 5, 1, 4, 6, 2, 9, 7, 3],
                [4, 8, 6, 9, 7, 5, 3, 1, 2],
                [5, 3, 7, 6, 2, 1, 4, 8, 9],
                [1, 9, 2, 8, 4, 3, 7, 6, 5],
                [6, 1, 8, 3, 5, 4, 2, 9, 7],
                [9, 7, 4, 2, 8, 6, 5, 3, 1],
                [3, 2, 5, 1, 9, 7, 8, 4, 6],
            ],
        };
        let params = Params::new(K);

        let vk = plonk::keygen_vk(&params, &sudoku).unwrap();

        let mut _vp = SudokuVP::new(sudoku, input_notes, output_notes);

        let vp_desc = ValidityPredicateVerifyingKey::from_vk(vk);

        let app_data = pallas::Base::zero();
        let app_data_dynamic = pallas::Base::zero();

        let value: u64 = 0;
        let nk_com = NullifierKeyCom::default();
        let rcm = pallas::Scalar::random(&mut rng);
        let psi = pallas::Base::random(&mut rng);
        let rho = Nullifier::new(pallas::Base::random(&mut rng));
        Note::new(
            vp_desc,
            app_data,
            app_data_dynamic,
            value,
            nk_com,
            rho,
            psi,
            rcm,
            true,
        );
    }
}