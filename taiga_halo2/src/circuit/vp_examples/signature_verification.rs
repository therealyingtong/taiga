use crate::{
    circuit::{
        blake2s::publicize_default_dynamic_vp_commitments,
        gadgets::{
            assign_free_advice, poseidon_hash::poseidon_hash_gadget,
            target_note_variable::get_owned_note_variable,
        },
        vp_circuit::{
            BasicValidityPredicateVariables, VPVerifyingInfo, ValidityPredicateCircuit,
            ValidityPredicateConfig, ValidityPredicatePublicInputs, ValidityPredicateVerifyingInfo,
        },
    },
    constant::{TaigaFixedBasesFull, NUM_NOTE, SETUP_PARAMS_MAP},
    note::{Note, RandomSeed},
    proof::Proof,
    utils::{mod_r_p, poseidon_hash_n},
    vp_commitment::ValidityPredicateCommitment,
    vp_vk::ValidityPredicateVerifyingKey,
};
use halo2_gadgets::ecc::{chip::EccChip, FixedPoint, NonIdentityPoint, ScalarFixed, ScalarVar};
use halo2_proofs::{
    arithmetic::Field,
    circuit::{floor_planner, Layouter, Value},
    plonk::{keygen_pk, keygen_vk, Circuit, ConstraintSystem, Error},
};
use lazy_static::lazy_static;
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{Curve, Group},
    pallas,
};
use rand::rngs::OsRng;
use rand::RngCore;

// The message contains the input note nullifiers and output note commitments
const MESSAGE_LEN: usize = NUM_NOTE * 2;
const POSEIDON_HASH_LEN: usize = MESSAGE_LEN + 4;
lazy_static! {
    pub static ref TOKEN_AUTH_VK: ValidityPredicateVerifyingKey =
        SignatureVerificationValidityPredicateCircuit::default().get_vp_vk();
    pub static ref COMPRESSED_TOKEN_AUTH_VK: pallas::Base = TOKEN_AUTH_VK.get_compressed();
}

#[derive(Clone, Debug)]
pub struct SchnorrSignature {
    // public key
    pk: pallas::Point,
    // signature (r,s)
    r: pallas::Point,
    s: pallas::Scalar,
}

impl Default for SchnorrSignature {
    fn default() -> Self {
        Self {
            pk: pallas::Point::generator(),
            r: pallas::Point::generator(),
            s: pallas::Scalar::one(),
        }
    }
}

impl SchnorrSignature {
    pub fn sign<R: RngCore>(mut rng: R, sk: pallas::Scalar, message: Vec<pallas::Base>) -> Self {
        // TDOD: figure out whether the generator is applicable.
        let generator = pallas::Point::generator();
        let pk = generator * sk;
        let pk_coord = pk.to_affine().coordinates().unwrap();
        // Generate a random number: z
        let z = pallas::Scalar::random(&mut rng);
        // Compute: R = z*G
        let r = generator * z;
        let r_coord = r.to_affine().coordinates().unwrap();
        // Compute: s = z + Hash(r||P||m)*sk
        assert_eq!(message.len(), MESSAGE_LEN);
        let h = mod_r_p(poseidon_hash_n::<POSEIDON_HASH_LEN>([
            *r_coord.x(),
            *r_coord.y(),
            *pk_coord.x(),
            *pk_coord.y(),
            message[0],
            message[1],
            message[2],
            message[3],
        ]));
        let s = z + h * sk;
        Self { pk, r, s }
    }
}

// SignatureVerificationValidityPredicateCircuit uses the schnorr signature.
#[derive(Clone, Debug, Default)]
pub struct SignatureVerificationValidityPredicateCircuit {
    pub owned_note_pub_id: pallas::Base,
    pub input_notes: [Note; NUM_NOTE],
    pub output_notes: [Note; NUM_NOTE],
    pub vp_vk: pallas::Base,
    pub signature: SchnorrSignature,
    pub receiver_vp_vk: pallas::Base,
}

impl SignatureVerificationValidityPredicateCircuit {
    pub fn new(
        owned_note_pub_id: pallas::Base,
        input_notes: [Note; NUM_NOTE],
        output_notes: [Note; NUM_NOTE],
        vp_vk: pallas::Base,
        signature: SchnorrSignature,
        receiver_vp_vk: pallas::Base,
    ) -> Self {
        Self {
            owned_note_pub_id,
            input_notes,
            output_notes,
            vp_vk,
            signature,
            receiver_vp_vk,
        }
    }

    pub fn from_sk_and_sign<R: RngCore>(
        mut rng: R,
        owned_note_pub_id: pallas::Base,
        input_notes: [Note; NUM_NOTE],
        output_notes: [Note; NUM_NOTE],
        vp_vk: pallas::Base,
        sk: pallas::Scalar,
        receiver_vp_vk: pallas::Base,
    ) -> Self {
        assert_eq!(NUM_NOTE, 2);
        let mut message = vec![];
        input_notes
            .iter()
            .zip(output_notes.iter())
            .for_each(|(input_note, output_note)| {
                let nf = input_note.get_nf().unwrap().inner();
                message.push(nf);
                let cm = output_note.commitment();
                message.push(cm.inner());
            });
        let signature = SchnorrSignature::sign(&mut rng, sk, message);
        Self {
            owned_note_pub_id,
            input_notes,
            output_notes,
            vp_vk,
            signature,
            receiver_vp_vk,
        }
    }
}

impl ValidityPredicateCircuit for SignatureVerificationValidityPredicateCircuit {
    // Add custom constraints
    fn custom_constraints(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<pallas::Base>,
        basic_variables: BasicValidityPredicateVariables,
    ) -> Result<(), Error> {
        // Construct an ECC chip
        let ecc_chip = EccChip::construct(config.ecc_config);

        let pk = NonIdentityPoint::new(
            ecc_chip.clone(),
            layouter.namespace(|| "witness pk"),
            Value::known(self.signature.pk.to_affine()),
        )?;

        // search target note and get the app_data_dynamic
        let owned_note_pub_id = basic_variables.get_owned_note_pub_id();
        let app_data_dynamic = get_owned_note_variable(
            config.get_owned_note_variable_config,
            layouter.namespace(|| "get owned note app_data_dynamic"),
            &owned_note_pub_id,
            &basic_variables.get_app_data_dynamic_searchable_pairs(),
        )?;

        let auth_vp_vk = assign_free_advice(
            layouter.namespace(|| "witness auth vp vk"),
            config.advices[0],
            Value::known(self.vp_vk),
        )?;
        let receiver_vp_vk = assign_free_advice(
            layouter.namespace(|| "witness receiver vp vk"),
            config.advices[0],
            Value::known(self.receiver_vp_vk),
        )?;

        // Decode the app_data_dynamic, and check the app_data_dynamic encoding
        let encoded_app_data_dynamic = poseidon_hash_gadget(
            config.poseidon_config.clone(),
            layouter.namespace(|| "app_data_dynamic encoding"),
            [pk.inner().x(), pk.inner().y(), auth_vp_vk, receiver_vp_vk],
        )?;

        layouter.assign_region(
            || "check app_data_dynamic encoding",
            |mut region| {
                region.constrain_equal(encoded_app_data_dynamic.cell(), app_data_dynamic.cell())
            },
        )?;

        let r = NonIdentityPoint::new(
            ecc_chip.clone(),
            layouter.namespace(|| "witness r"),
            Value::known(self.signature.r.to_affine()),
        )?;
        let s_scalar = ScalarFixed::new(
            ecc_chip.clone(),
            layouter.namespace(|| "witness s"),
            Value::known(self.signature.s),
        )?;

        // Verify: s*G = R + Hash(r||P||m)*P
        // s*G
        let generator =
            FixedPoint::from_inner(ecc_chip.clone(), TaigaFixedBasesFull::BaseGenerator);
        let (s_g, _) = generator.mul(layouter.namespace(|| "s_scalar * generator"), &s_scalar)?;

        // Hash(r||P||m)
        let h_scalar = {
            let nfs = basic_variables.get_input_note_nfs();
            let cms = basic_variables.get_output_note_cms();
            assert_eq!(NUM_NOTE, 2);
            let h = poseidon_hash_gadget(
                config.poseidon_config,
                layouter.namespace(|| "Poseidon_hash(r, P, m)"),
                [
                    r.inner().x(),
                    r.inner().y(),
                    pk.inner().x(),
                    pk.inner().y(),
                    nfs[0].clone(),
                    cms[0].clone(),
                    nfs[1].clone(),
                    cms[1].clone(),
                ],
            )?;

            ScalarVar::from_base(ecc_chip, layouter.namespace(|| "ScalarVar from_base"), &h)?
        };

        // Hash(r||P||m)*P
        let (h_p, _) = pk.mul(layouter.namespace(|| "hP"), h_scalar)?;

        // R + Hash(r||P||m)*P
        let rhs = r.add(layouter.namespace(|| "R + Hash(r||P||m)*P"), &h_p)?;

        s_g.constrain_equal(layouter.namespace(|| "s*G = R + Hash(r||P||m)*P"), &rhs)?;

        // Publicize the dynamic vp commitments with default value
        publicize_default_dynamic_vp_commitments(
            &mut layouter,
            config.advices[0],
            config.instances,
        )?;

        Ok(())
    }

    fn get_input_notes(&self) -> &[Note; NUM_NOTE] {
        &self.input_notes
    }

    fn get_output_notes(&self) -> &[Note; NUM_NOTE] {
        &self.output_notes
    }

    fn get_public_inputs(&self, mut rng: impl RngCore) -> ValidityPredicatePublicInputs {
        let mut public_inputs = self.get_mandatory_public_inputs();
        let default_vp_cm: [pallas::Base; 2] =
            ValidityPredicateCommitment::default().to_public_inputs();
        public_inputs.extend(default_vp_cm);
        public_inputs.extend(default_vp_cm);
        let padding = ValidityPredicatePublicInputs::get_public_input_padding(
            public_inputs.len(),
            &RandomSeed::random(&mut rng),
        );
        public_inputs.extend(padding);
        public_inputs.into()
    }

    fn get_owned_note_pub_id(&self) -> pallas::Base {
        self.owned_note_pub_id
    }
}

vp_circuit_impl!(SignatureVerificationValidityPredicateCircuit);
vp_verifying_info_impl!(SignatureVerificationValidityPredicateCircuit);

#[test]
fn test_halo2_sig_verification_vp_circuit() {
    use crate::circuit::vp_examples::{
        receiver_vp::COMPRESSED_RECEIVER_VK, token::TokenAuthorization,
    };
    use crate::constant::VP_CIRCUIT_PARAMS_SIZE;
    use crate::note::tests::{random_input_note, random_output_note};
    use halo2_proofs::dev::MockProver;
    use rand::rngs::OsRng;

    let mut rng = OsRng;
    let circuit = {
        let mut input_notes = [(); NUM_NOTE].map(|_| random_input_note(&mut rng));
        let output_notes = input_notes
            .iter()
            .map(|input| random_output_note(&mut rng, input.get_nf().unwrap()))
            .collect::<Vec<_>>();
        let sk = pallas::Scalar::random(&mut rng);
        let auth_vk = pallas::Base::random(&mut rng);
        let auth = TokenAuthorization::from_sk_vk(&sk, &auth_vk);
        input_notes[0].app_data_dynamic = auth.to_app_data_dynamic();
        let owned_note_pub_id = input_notes[0].get_nf().unwrap().inner();
        SignatureVerificationValidityPredicateCircuit::from_sk_and_sign(
            &mut rng,
            owned_note_pub_id,
            input_notes,
            output_notes.try_into().unwrap(),
            auth_vk,
            sk,
            *COMPRESSED_RECEIVER_VK,
        )
    };
    let public_inputs = circuit.get_public_inputs(&mut rng);

    let prover = MockProver::<pallas::Base>::run(
        VP_CIRCUIT_PARAMS_SIZE,
        &circuit,
        vec![public_inputs.to_vec()],
    )
    .unwrap();
    assert_eq!(prover.verify(), Ok(()));
}
