# History popup pointer ownership

The floating history/results/actions/Forget-confirmation panel may cover a live
terminal. A background alone does not occlude GPUI hitboxes. Without pointer
occlusion, pressing a popup control also starts a terminal gesture and moves
keyboard focus to the child; a window-level release handler can then consume the
control's release before its click is delivered.

Place `.occlude()` immediately after the panel's stable ID. All pointer events,
including wheel events during confirmation, belong to the visible popup rather
than the covered terminal. Outside clicks must still dismiss the popup and
follow normal routing.

This regression detector rejects the previous ID-to-key-context chain. It is
not a general layering proof; the GPUI interaction regression and full Linux
smoke test exercise click-through, focus, wheel isolation and confirmed Forget.
