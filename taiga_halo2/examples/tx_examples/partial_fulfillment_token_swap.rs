/// The token swap intent can be partially fulfilled.
/// Alice has 2 "BTC" and wants 10 "ETH". Then Alice creates an intent for it.
/// Bob has 5 "ETH" and wants 1 "BTC".
/// The Solver/Bob can partially fulfill Alice's intent and return 1 "BTC" back to Alice.
///
use crate::token::create_token_swap_ptx;
use group::Group;
use halo2_proofs::arithmetic::Field;
use pasta_curves::{group::Curve, pallas};
use rand::{CryptoRng, RngCore};
use taiga_halo2::{
    circuit::vp_examples::{
        partial_fulfillment_intent::{PartialFulfillmentIntentValidityPredicateCircuit, Swap},
        signature_verification::COMPRESSED_TOKEN_AUTH_VK,
        token::{Token, TokenAuthorization, TokenNote},
    },
    constant::TAIGA_COMMITMENT_TREE_DEPTH,
    merkle_tree::{Anchor, MerklePath},
    note::{InputNoteProvingInfo, Note, OutputNoteProvingInfo},
    nullifier::NullifierKeyContainer,
    shielded_ptx::ShieldedPartialTransaction,
    transaction::{ShieldedPartialTxBundle, Transaction, TransparentPartialTxBundle},
};

pub fn create_token_intent_ptx<R: RngCore>(
    mut rng: R,
    sell: Token,
    buy: Token,
    input_auth_sk: pallas::Scalar,
) -> (ShieldedPartialTransaction, Swap) {
    let input_auth = TokenAuthorization::from_sk_vk(&input_auth_sk, &COMPRESSED_TOKEN_AUTH_VK);
    let swap = Swap::random(&mut rng, sell, buy, input_auth);
    let intent_note = swap.create_intent_note(&mut rng);

    // padding the zero notes
    let padding_input_note = Note::random_padding_input_note(&mut rng);
    let padding_input_note_nf = padding_input_note.get_nf().unwrap();
    let padding_output_note = Note::random_padding_output_note(&mut rng, padding_input_note_nf);

    let input_notes = [*swap.sell.note(), padding_input_note];
    let output_notes = [intent_note, padding_output_note];

    let merkle_path = MerklePath::random(&mut rng, TAIGA_COMMITMENT_TREE_DEPTH);

    // Fetch a valid anchor for dummy notes
    let anchor = Anchor::from(pallas::Base::random(&mut rng));

    // Create the input note proving info
    let input_note_proving_info = swap.sell.generate_input_token_note_proving_info(
        &mut rng,
        input_auth,
        input_auth_sk,
        merkle_path.clone(),
        input_notes,
        output_notes,
    );

    // Create the intent note proving info
    let intent_note_proving_info = {
        let intent_vp = PartialFulfillmentIntentValidityPredicateCircuit {
            owned_note_pub_id: intent_note.commitment().inner(),
            input_notes,
            output_notes,
            swap: swap.clone(),
        };

        OutputNoteProvingInfo::new(intent_note, Box::new(intent_vp), vec![])
    };

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
    let ptx = ShieldedPartialTransaction::build(
        [input_note_proving_info, padding_input_note_proving_info],
        [intent_note_proving_info, padding_output_note_proving_info],
        vec![],
        &mut rng,
    );

    (ptx, swap)
}

#[allow(clippy::too_many_arguments)]
pub fn consume_token_intent_ptx<R: RngCore>(
    mut rng: R,
    swap: Swap,
    offer: Token,
    output_auth_pk: pallas::Point,
) -> ShieldedPartialTransaction {
    let intent_note = swap.create_intent_note(&mut rng);
    let (input_notes, output_notes) = swap.fill(&mut rng, intent_note, offer);
    let [intent_note, padding_input_note] = input_notes;
    let [bought_note, returned_note] = output_notes;

    // output notes
    let output_auth = TokenAuthorization::new(output_auth_pk, *COMPRESSED_TOKEN_AUTH_VK);
    let merkle_path = MerklePath::random(&mut rng, TAIGA_COMMITMENT_TREE_DEPTH);

    // Fetch a valid anchor for dummy notes
    let anchor = Anchor::from(pallas::Base::random(&mut rng));

    // Create the intent note proving info
    let intent_note_proving_info = {
        let intent_vp = PartialFulfillmentIntentValidityPredicateCircuit {
            owned_note_pub_id: intent_note.get_nf().unwrap().inner(),
            input_notes,
            output_notes,
            swap: swap.clone(),
        };

        InputNoteProvingInfo::new(
            intent_note,
            merkle_path.clone(),
            Some(anchor),
            Box::new(intent_vp),
            vec![],
        )
    };

    // Create the output note proving info
    let bought_note_proving_info = TokenNote {
        token_name: swap.buy.name().clone(),
        note: bought_note,
    }
    .generate_output_token_note_proving_info(&mut rng, output_auth, input_notes, output_notes);

    // Create the padding input note proving info
    let padding_input_note_proving_info = InputNoteProvingInfo::create_padding_note_proving_info(
        padding_input_note,
        merkle_path,
        anchor,
        input_notes,
        output_notes,
    );

    // Create the returned note proving info
    let returned_note_proving_info = TokenNote {
        token_name: swap.sell.token_name().clone(),
        note: returned_note,
    }
    .generate_output_token_note_proving_info(&mut rng, output_auth, input_notes, output_notes);

    // Create shielded partial tx
    ShieldedPartialTransaction::build(
        [intent_note_proving_info, padding_input_note_proving_info],
        [bought_note_proving_info, returned_note_proving_info],
        vec![],
        &mut rng,
    )
}

pub fn create_token_swap_transaction<R: RngCore + CryptoRng>(mut rng: R) -> Transaction {
    let generator = pallas::Point::generator().to_affine();

    // Alice creates the partial transaction with:
    // - 2 BTC sell
    // - intent output encoding 10 ETH ask
    let alice_auth_sk = pallas::Scalar::random(&mut rng);
    let alice_auth_pk = generator * alice_auth_sk;
    let sell = Token::new("btc".to_string(), 2u64);
    let buy = Token::new("eth".to_string(), 10u64);
    let (alice_ptx, swap) =
        create_token_intent_ptx(&mut rng, sell.clone(), buy.clone(), alice_auth_sk);

    // Bob creates the partial transaction with 1 DOLPHIN input and 5 BTC output
    let bob_auth_sk = pallas::Scalar::random(&mut rng);
    let bob_auth_pk = generator * bob_auth_sk;
    let bob_nk = NullifierKeyContainer::random_key(&mut rng);
    let offer = Token::new("eth".to_string(), 5);
    let returned = Token::new("btc".to_string(), 1);

    let bob_ptx = create_token_swap_ptx(
        &mut rng,
        offer.clone(),
        bob_auth_sk,
        bob_nk,
        returned,
        bob_auth_pk,
        bob_nk.to_commitment(),
    );

    // Solver/Bob creates the partial transaction to consume the intent note
    // The bob_ptx and solver_ptx can be merged to one ptx.
    let solver_ptx = consume_token_intent_ptx(&mut rng, swap, offer, alice_auth_pk);

    // Solver creates the final transaction
    let shielded_tx_bundle = ShieldedPartialTxBundle::new(vec![alice_ptx, bob_ptx, solver_ptx]);
    let transparent_ptx_bundle = TransparentPartialTxBundle::default();
    Transaction::build(&mut rng, shielded_tx_bundle, transparent_ptx_bundle)
}

#[test]
fn test_partial_fulfillment_token_swap_tx() {
    use rand::rngs::OsRng;

    let mut rng = OsRng;
    let tx = create_token_swap_transaction(&mut rng);
    tx.execute().unwrap();
}
