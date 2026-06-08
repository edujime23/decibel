function initializeCoreMod() {
    return {
        'decibel_library_transformer': {
            'target': {
                'type': 'CLASS',
                'name': 'com.mojang.blaze3d.audio.Library'
            },
            'transformer': function(classNode) {
                var Opcodes = Java.type('org.objectweb.asm.Opcodes');
                var InsnList = Java.type('org.objectweb.asm.tree.InsnList');
                var VarInsnNode = Java.type('org.objectweb.asm.tree.VarInsnNode');
                var MethodInsnNode = Java.type('org.objectweb.asm.tree.MethodInsnNode');
                var InsnNode = Java.type('org.objectweb.asm.tree.InsnNode');

                print("[Decibel CoreMod] Transforming com.mojang.blaze3d.audio.Library safely...");

                var initMethod = null;
                var cleanupMethod = null;
                for (var i = 0; i < classNode.methods.size(); i++) {
                    var m = classNode.methods.get(i);
                    if (m.name === "init" && m.desc === "(Ljava/lang/String;Z)V") {
                        initMethod = m;
                    } else if (m.name === "cleanup" && m.desc === "()V") {
                        cleanupMethod = m;
                    }
                }

                if (initMethod !== null) {
                    var newInsns = new InsnList();
                    newInsns.add(new VarInsnNode(Opcodes.ALOAD, 0)); // Pass 'this' (Library instance)
                    newInsns.add(new VarInsnNode(Opcodes.ALOAD, 1)); // Device name
                    newInsns.add(new VarInsnNode(Opcodes.ILOAD, 2)); // useNoAudio
                    newInsns.add(new MethodInsnNode(
                        Opcodes.INVOKESTATIC,
                        "com/edujime23/decibel/virtual/VirtualLibrary",
                        "init",
                        "(Lcom/mojang/blaze3d/audio/Library;Ljava/lang/String;Z)V",
                        false
                    ));
                    newInsns.add(new InsnNode(Opcodes.RETURN));

                    initMethod.tryCatchBlocks.clear();
                    if (initMethod.localVariables !== null) {
                        initMethod.localVariables.clear();
                    }
                    initMethod.instructions.clear();
                    initMethod.instructions.add(newInsns);
                }

                if (cleanupMethod !== null) {
                    var cleanInsns = new InsnList();
                    cleanInsns.add(new MethodInsnNode(
                        Opcodes.INVOKESTATIC,
                        "com/edujime23/decibel/virtual/VirtualLibrary",
                        "cleanup",
                        "()V",
                        false
                    ));
                    cleanInsns.add(new InsnNode(Opcodes.RETURN));

                    cleanupMethod.tryCatchBlocks.clear();
                    if (cleanupMethod.localVariables !== null) {
                        cleanupMethod.localVariables.clear();
                    }
                    cleanupMethod.instructions.clear();
                    cleanupMethod.instructions.add(cleanInsns);
                }

                return classNode;
            }
        }
    };
}