use crate::halo2::{
    constant::TAIGA_COMMITMENT_TREE_DEPTH,
    merkle_tree::{MerklePath, Node},
    note::{Note, NoteCommitment},
    nullifier::Nullifier,
    token::Token,
    user::User,
    user::UserSendAddress,
    vp_description::ValidityPredicateDescription,
};
use ff::Field;
use pasta_curves::pallas;
use rand::RngCore;

/// The action result used in transaction.
#[derive(Copy, Debug, Clone)]
pub struct Action {
    /// The root of the note commitment Merkle tree.
    pub root: pallas::Base,
    /// The nullifier of the spend note.
    pub nf: Nullifier,
    /// The commitment of the output note.
    pub cm: NoteCommitment,
    // TODO: The EncryptedNote.
    // encrypted_note,
}

/// The information to build Action and ActionCircuit.
#[derive(Debug, Clone)]
pub struct ActionInfo {
    spend: SpendInfo,
    output: OutputInfo,
}

#[derive(Debug, Clone)]
pub struct SpendInfo {
    note: Note,
    auth_path: [(pallas::Base, bool); TAIGA_COMMITMENT_TREE_DEPTH],
    root: pallas::Base,
}

#[derive(Debug, Clone)]
pub struct OutputInfo {
    addr_send_closed: UserSendAddress,
    addr_recv_vp: ValidityPredicateDescription,
    addr_token_vp: ValidityPredicateDescription,
    value: u64,
    data: pallas::Base,
}

impl ActionInfo {
    pub fn new(spend: SpendInfo, output: OutputInfo) -> Self {
        Self { spend, output }
    }

    pub fn dummy<R: RngCore>(mut rng: R) -> Self {
        let spend_note = Note::dummy(&mut rng);
        let merkle_path = MerklePath::dummy(&mut rng, TAIGA_COMMITMENT_TREE_DEPTH);
        let spend_info = SpendInfo::new(spend_note, merkle_path);

        let output_info = OutputInfo::dummy(&mut rng);

        ActionInfo::new(spend_info, output_info)
    }

    pub fn build(
        self,
        rng: &mut impl RngCore,
        // ) -> (Action, ActionCircuit) {
    ) -> Action {
        let spend_cm = self.spend.note.commitment();
        let nk = self.spend.note.user.send_com.get_nk().unwrap();
        let nf = Nullifier::derive_native(
            &nk,
            &self.spend.note.rho.inner(),
            &self.spend.note.psi,
            &spend_cm,
        );

        let user = User {
            send_com: self.output.addr_send_closed,
            recv_vp: self.output.addr_recv_vp,
        };
        let token = Token {
            token_vp: self.output.addr_token_vp,
        };

        let note_rcm = pallas::Scalar::random(rng);
        let output_note = Note::new(
            user,
            token,
            self.output.value,
            nf,
            self.output.data,
            note_rcm,
        );

        let output_cm = output_note.commitment();
        Action {
            nf,
            cm: output_cm,
            root: self.spend.root,
        }

        // let action_circuit = ActionCircuit{
        //     spend_note: self.spend.note,
        //     auth_path: self.spend.auth_path,
        //     output_note,
        // };

        // (action, action_circuit)
    }
}

impl SpendInfo {
    pub fn new(note: Note, merkle_path: MerklePath) -> Self {
        let cm_node = Node::new(note.commitment().get_x());
        let root = merkle_path.root(cm_node).inner();
        let auth_path: [(pallas::Base, bool); TAIGA_COMMITMENT_TREE_DEPTH] =
            merkle_path.get_path().as_slice().try_into().unwrap();
        Self {
            note,
            auth_path,
            root,
        }
    }
}

impl OutputInfo {
    pub fn new(
        addr_send_closed: UserSendAddress,
        addr_recv_vp: ValidityPredicateDescription,
        addr_token_vp: ValidityPredicateDescription,
        value: u64,
        data: pallas::Base,
    ) -> Self {
        Self {
            addr_send_closed,
            addr_recv_vp,
            addr_token_vp,
            value,
            data,
        }
    }

    pub fn dummy<R: RngCore>(mut rng: R) -> Self {
        use rand::Rng;
        let addr_send_closed = UserSendAddress::from_closed(pallas::Base::random(&mut rng));
        let addr_recv_vp = ValidityPredicateDescription::dummy(&mut rng);
        let addr_token_vp = ValidityPredicateDescription::dummy(&mut rng);
        let value: u64 = rng.gen();
        let data = pallas::Base::random(rng);
        Self {
            addr_send_closed,
            addr_recv_vp,
            addr_token_vp,
            value,
            data,
        }
    }
}