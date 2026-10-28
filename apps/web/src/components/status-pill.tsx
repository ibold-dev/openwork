import type { AgreementState, MilestoneState } from '@openwork/sdk';

const MILESTONE_LABELS: Record<MilestoneState, string> = {
  pending: 'Awaiting funding',
  funded: 'Funded',
  submitted: 'Submitted',
  released: 'Released',
  refunded: 'Refunded',
  disputed: 'Disputed',
};

const MILESTONE_TONE: Record<MilestoneState, string> = {
  pending: '',
  funded: 'pill-good',
  submitted: 'pill-accent',
  released: 'pill-good',
  refunded: 'pill-warn',
  disputed: 'pill-bad',
};

const AGREEMENT_LABELS: Record<AgreementState, string> = {
  draft: 'Draft',
  active: 'Active',
  completed: 'Completed',
  cancelled: 'Cancelled',
};

const AGREEMENT_TONE: Record<AgreementState, string> = {
  draft: '',
  active: 'pill-good',
  completed: 'pill-good',
  cancelled: 'pill-warn',
};

export function MilestoneStatus({ state }: { state: MilestoneState }) {
  return (
    <span className={`pill ${MILESTONE_TONE[state]}`}>
      <span className="pill-dot" />
      {MILESTONE_LABELS[state]}
    </span>
  );
}

export function AgreementStatus({ state }: { state: AgreementState }) {
  return (
    <span className={`pill ${AGREEMENT_TONE[state]}`}>
      <span className="pill-dot" />
      {AGREEMENT_LABELS[state]}
    </span>
  );
}
