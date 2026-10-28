'use client';

import { motion, useReducedMotion } from 'motion/react';

import { MilestoneStatus } from './status-pill';

const ROWS = [
  {
    title: 'Design system',
    sub: 'Milestone 01 · approved',
    amount: '1,200.00',
    state: 'released' as const,
  },
  {
    title: 'Marketing site',
    sub: 'Milestone 02 · in review',
    amount: '2,400.00',
    state: 'submitted' as const,
  },
  {
    title: 'Analytics wiring',
    sub: 'Milestone 03 · funded',
    amount: '900.00',
    state: 'funded' as const,
  },
];

/**
 * Illustrative ledger used on the landing page. It is clearly labelled as an
 * example; no numbers here are presented as live data.
 */
export function LedgerVisual() {
  const reduceMotion = useReducedMotion();

  return (
    <div className="ledger" aria-label="Example agreement ledger">
      <div className="ledger-head">
        <div>
          <div className="ledger-title">agreement / 7Kd2…m9Qf</div>
          <div className="tiny faint" style={{ marginTop: 2 }}>
            client 4Nd1…DB4T · freelancer 9WzD…AWWM
          </div>
        </div>
        <span className="pill pill-good">
          <span className="pill-dot" />
          active
        </span>
      </div>

      <div className="ledger-body">
        {ROWS.map((row, index) => (
          <motion.div
            key={row.title}
            className="ledger-row"
            initial={reduceMotion ? false : { opacity: 0, x: -8 }}
            animate={{ opacity: 1, x: 0 }}
            transition={{
              duration: 0.5,
              delay: 0.15 + index * 0.12,
              ease: [0.22, 1, 0.36, 1],
            }}
          >
            <span className="ledger-index">0{index + 1}</span>
            <div>
              <div className="ledger-row-title">{row.title}</div>
              <div className="ledger-row-sub">{row.sub}</div>
            </div>
            <div style={{ display: 'grid', justifyItems: 'end', gap: 4 }}>
              <span className="ledger-amount gradient-text">{row.amount}</span>
              <MilestoneStatus state={row.state} />
            </div>
          </motion.div>
        ))}
      </div>

      <div className="ledger-foot">
        <span>vault holds 900.00 USDC</span>
        <span className="mono">fee 0.00%</span>
      </div>

      <div style={{ padding: '0 1.15rem 1.1rem' }}>
        <div className="rail-track" aria-hidden="true">
          <motion.div
            className="rail-fill"
            initial={reduceMotion ? { width: '58%' } : { width: 0 }}
            animate={{ width: '58%' }}
            transition={{ duration: 0.9, delay: 0.6, ease: [0.22, 1, 0.36, 1] }}
          />
        </div>
      </div>
    </div>
  );
}
