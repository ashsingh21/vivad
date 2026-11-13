# Fixing a pose by eye

Some things only a person can see: a hand a little off the gun in the slide, a head that sits
wrong in one clip. `crates/game/src/posefix.rs` lets you move the body part where it should be,
in the running game, and keeps the move as data that is laid over that movement from then on.

## Making a fix

```
just posefix
```

1. Play until something looks wrong. **F7** is slow motion, to see it coming.
2. **F8** stops time and frees the pointer. A dot sits on the hips, chest, head, each hand, each
   foot, and one between the hands (both hands together, with what they hold).
3. Look round: right-drag or the arrow keys, the wheel to come closer. **.** steps one frame on.
4. Drag a dot. The part follows the pointer in the plane facing the camera: the elbow or knee
   bends to reach, lowered hips bend the knees and leave the feet, and "both hands" keeps the left
   hand in its place on the right.
5. Let go: `assets/data/pose_fixes.ron` is written. **F8** again carries on playing, and the
   fix is there every time that movement plays.

The panel says what is showing (the legs' or whole-body clip, and the arms' action over it) and
what the fix is being saved for. **Tab** changes that: this clip under this action, this clip
whatever the arms do, or this action whatever the legs do. **Backspace** clears the fix for the
scope shown.

## What a fix is

```ron
[
    (clip: "rifle_slide", action: "*", parts: {Hands: (0.0, 0.08, 0.02)}),
]
```

A part (`Hips`, `Chest`, `Head`, `Hands`, `HandL`, `HandR`, `FootL`, `FootR`) moved by metres in
the character's model space (+X its left, +Y up, +Z forward), for a clip and an action as
`WarriorAnim::showing` names them (`"*"` is any). Every warrior gets it, bots and other players
too, in every build including the browser's (the file is compiled in; on the desktop the copy on
disk is read at start). It eases in and out with the clips. Edit the file by hand if you like.

It runs last: after the clips, the torso's aim, and the weapon and left-hand placement in
`crate::arms`. So a fix moves what those produced and is not corrected back.

## Limits

- An offset, not a keyframe: the same move holds for as long as the clip shows. It can't shape
  the start of a slide differently from its hold.
- Moves, no twists: a hand or foot keeps the way it faces.
- A limb stops at full stretch, so a long drag may move the part less than the pointer.
- Solo only: stopping time stops this machine's sim, not a hosted match.
- A fix for a clip is the same for every body (male, female, anime).

`cargo test -p game posefix` checks the moves on a stick figure (a hand lands where it is
dragged, both hands move as one, lowered hips leave the feet).
