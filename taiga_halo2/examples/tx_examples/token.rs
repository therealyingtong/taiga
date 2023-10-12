use halo2_proofs::arithmetic::Field;

use pasta_curves::pallas;
use rand::RngCore;

use taiga_halo2::{
    circuit::vp_examples::{
        signature_verification::COMPRESSED_TOKEN_AUTH_VK,
        token::{
            generate_input_token_note_proving_info, generate_output_token_note_proving_info, Token,
            TokenAuthorization, COMPRESSED_TOKEN_VK,
        },
    },
    constant::TAIGA_COMMITMENT_TREE_DEPTH,
    merkle_tree::{Anchor, MerklePath},
    note::{InputNoteProvingInfo, Note, OutputNoteProvingInfo, RandomSeed},
    nullifier::{Nullifier, NullifierKeyContainer},
    shielded_ptx::ShieldedPartialTransaction,
};

pub fn create_random_token_note<R: RngCore>(
    mut rng: R,
    token: &Token,
    rho: Nullifier,
    nk_container: NullifierKeyContainer,
    auth: &TokenAuthorization,
) -> Note {
    let app_data_static = token.encode_name();
    let app_data_dynamic = auth.to_app_data_dynamic();
    let rseed = RandomSeed::random(&mut rng);
    Note::new(
        *COMPRESSED_TOKEN_VK,
        app_data_static,
        app_data_dynamic,
        token.value(),
        nk_container,
        rho,
        true,
        rseed,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn create_token_swap_ptx<R: RngCore>(
    mut rng: R,
    input_token: Token,
    input_auth_sk: pallas::Scalar,
    input_nk: NullifierKeyContainer, // NullifierKeyContainer::Key
    output_token: Token,
    output_auth_pk: pallas::Point,
    output_nk_com: NullifierKeyContainer, // NullifierKeyContainer::Commitment
) -> ShieldedPartialTransaction {
    let input_auth = TokenAuthorization::from_sk_vk(&input_auth_sk, &COMPRESSED_TOKEN_AUTH_VK);

    // input note
    let rho = Nullifier::from(pallas::Base::random(&mut rng));
    let input_note = create_random_token_note(&mut rng, &input_token, rho, input_nk, &input_auth);

    // output note
    let input_note_nf = input_note.get_nf().unwrap();
    let output_auth = TokenAuthorization::new(output_auth_pk, *COMPRESSED_TOKEN_AUTH_VK);
    let output_note = create_random_token_note(
        &mut rng,
        &output_token,
        input_note_nf,
        output_nk_com,
        &output_auth,
    );

    // padding the zero notes
    let padding_input_note = Note::random_padding_input_note(&mut rng);
    let padding_input_note_nf = padding_input_note.get_nf().unwrap();
    let padding_output_note = Note::random_padding_output_note(&mut rng, padding_input_note_nf);

    let input_notes = [input_note, padding_input_note];
    let output_notes = [output_note, padding_output_note];

    // Generate proving info
    let merkle_path = MerklePath::random(&mut rng, TAIGA_COMMITMENT_TREE_DEPTH);

    // Fetch a valid anchor for padding input notes
    let anchor = Anchor::from(pallas::Base::random(&mut rng));

    // Create the input note proving info
    let input_note_proving_info = generate_input_token_note_proving_info(
        &mut rng,
        input_note,
        input_token.name(),
        input_auth,
        input_auth_sk,
        merkle_path.clone(),
        input_notes,
        output_notes,
    );

    // Create the output note proving info
    let output_note_proving_info = generate_output_token_note_proving_info(
        &mut rng,
        output_note,
        output_token.name(),
        output_auth,
        input_notes,
        output_notes,
    );

    // Create the padding input note proving info
    let padding_input_note_proving_info = InputNoteProvingInfo::create_padding_note_proving_info(
        padding_input_note,
        merkle_path,
        anchor,
        input_notes,
        output_notes,
    );

    // Create the padding output note proving info
    let padding_output_note_proving_info = OutputNoteProvingInfo::create_padding_note_proving_info(
        padding_output_note,
        input_notes,
        output_notes,
    );

    // Create shielded partial tx
    ShieldedPartialTransaction::build(
        [input_note_proving_info, padding_input_note_proving_info],
        [output_note_proving_info, padding_output_note_proving_info],
        vec![],
        &mut rng,
    )
}
