# Retained collection growth

AGENTS.md: bound retained data and surface truncation/gaps. Review structural pushes to self.events/self.blocks and self.output.extend_from_slice in canonical recording.rs or document model.rs. Before tightening this matched 7 sites: 4 retained writes and 3 false positives (scanner.push and two out.push calls in a derived text string); now 4 retained writes. Existing event, block and output limits are checked before append, but raw-positive review signals remain. It cannot prove bounds; accepted decisions should watch the owning source.
