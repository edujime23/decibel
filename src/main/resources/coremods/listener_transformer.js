function initializeCoreMod() {
    return {
        'decibel_listener_transformer': {
            'target': {
                'type': 'CLASS',
                'name': 'com.mojang.blaze3d.audio.Listener'
            },
            'transformer': function(classNode) {
                var Opcodes = Java.type('org.objectweb.asm.Opcodes');
                var InsnList = Java.type('org.objectweb.asm.tree.InsnList');
                var MethodInsnNode = Java.type('org.objectweb.asm.tree.MethodInsnNode');
                var InsnNode = Java.type('org.objectweb.asm.tree.InsnNode');

                print("[Decibel CoreMod] Transforming com.mojang.blaze3d.audio.Listener safely...");

                for (var i = 0; i < classNode.methods.size(); i++) {
                    var m = classNode.methods.get(i);
                    var name = m.name;
                    var desc = m.desc;

                    if (name === "reset" && desc === "()V") {
                        var insns = new InsnList();
                        insns.add(new MethodInsnNode(
                            Opcodes.INVOKESTATIC,
                            "com/edujime23/decibel/virtual/VirtualListener",
                            "reset",
                            "()V",
                            false
                        ));
                        insns.add(new InsnNode(Opcodes.RETURN));

                        m.tryCatchBlocks.clear();
                        if (m.localVariables !== null) {
                            m.localVariables.clear();
                        }
                        m.instructions.clear();
                        m.instructions.add(insns);
                    } else if (name === "setListenerPosition" || name === "setListenerOrientation" || name === "setGain" || name === "setTransform") {
                        // Position & Transform sync is handled inside ClientTickHandler / SoundInterceptor
                        var insns = new InsnList();
                        insns.add(new InsnNode(Opcodes.RETURN));

                        m.tryCatchBlocks.clear();
                        if (m.localVariables !== null) {
                            m.localVariables.clear();
                        }
                        m.instructions.clear();
                        m.instructions.add(insns);
                    }
                }
                return classNode;
            }
        }
    };
}