function initializeCoreMod() {
    return {
        'decibel_channel_transformer': {
            'target': {
                'type': 'CLASS',
                'name': 'com.mojang.blaze3d.audio.Channel'
            },
            'transformer': function(classNode) {
                var Opcodes = Java.type('org.objectweb.asm.Opcodes');
                var InsnList = Java.type('org.objectweb.asm.tree.InsnList');
                var VarInsnNode = Java.type('org.objectweb.asm.tree.VarInsnNode');
                var MethodInsnNode = Java.type('org.objectweb.asm.tree.MethodInsnNode');
                var TypeInsnNode = Java.type('org.objectweb.asm.tree.TypeInsnNode');
                var FieldInsnNode = Java.type('org.objectweb.asm.tree.FieldInsnNode');
                var InsnNode = Java.type('org.objectweb.asm.tree.InsnNode');

                print("[Decibel CoreMod] Transforming com.mojang.blaze3d.audio.Channel comprehensively...");

                for (var i = 0; i < classNode.methods.size(); i++) {
                    var m = classNode.methods.get(i);
                    var name = m.name;
                    var desc = m.desc;

                    var insns = new InsnList();
                    var replaced = false;

                    if (name === "<init>" && desc === "(I)V") {
                        insns.add(new VarInsnNode(Opcodes.ALOAD, 0));
                        insns.add(new MethodInsnNode(Opcodes.INVOKESPECIAL, "java/lang/Object", "<init>", "()V", false));

                        // Find the first NON-STATIC integer field
                        var intField = null;
                        for (var k = 0; k < classNode.fields.size(); k++) {
                            var f = classNode.fields.get(k);
                            if (f.desc === "I" && (f.access & Opcodes.ACC_STATIC) === 0) {
                                intField = f.name;
                                break;
                            }
                        }

                        if (intField !== null) {
                            insns.add(new VarInsnNode(Opcodes.ALOAD, 0));
                            insns.add(new VarInsnNode(Opcodes.ILOAD, 1));
                            insns.add(new FieldInsnNode(Opcodes.PUTFIELD, "com/mojang/blaze3d/audio/Channel", intField, "I"));
                        }

                        insns.add(new VarInsnNode(Opcodes.ALOAD, 0));
                        insns.add(new MethodInsnNode(Opcodes.INVOKESTATIC, "com/edujime23/decibel/virtual/VirtualChannel", "register", "(Ljava/lang/Object;)V", false));
                        insns.add(new InsnNode(Opcodes.RETURN));
                        replaced = true;

                    } else if (name === "create" && desc === "()Lcom/mojang/blaze3d/audio/Channel;") {
                        insns.add(new TypeInsnNode(Opcodes.NEW, "com/mojang/blaze3d/audio/Channel"));
                        insns.add(new InsnNode(Opcodes.DUP));
                        insns.add(new InsnNode(Opcodes.ICONST_1));
                        insns.add(new MethodInsnNode(Opcodes.INVOKESPECIAL, "com/mojang/blaze3d/audio/Channel", "<init>", "(I)V", false));
                        insns.add(new InsnNode(Opcodes.ARETURN));
                        replaced = true;

                    } else if (desc === "()Z" && (name === "playing" || name === "stopped")) {
                        var methodName = name === "playing" ? "isPlaying" : "isStopped";
                        insns.add(new VarInsnNode(Opcodes.ALOAD, 0));
                        insns.add(new MethodInsnNode(Opcodes.INVOKESTATIC, "com/edujime23/decibel/virtual/VirtualChannel", methodName, "(Ljava/lang/Object;)Z", false));
                        insns.add(new InsnNode(Opcodes.IRETURN));
                        replaced = true;

                    } else if (desc === "()V" && ["play", "stop", "pause", "unpause", "release", "destroy", "updateStream", "disableAttenuation"].indexOf(name) !== -1) {
                        var targetName = name;
                        if (name === "destroy") targetName = "release";

                        insns.add(new VarInsnNode(Opcodes.ALOAD, 0));
                        insns.add(new MethodInsnNode(Opcodes.INVOKESTATIC, "com/edujime23/decibel/virtual/VirtualChannel", targetName, "(Ljava/lang/Object;)V", false));
                        insns.add(new InsnNode(Opcodes.RETURN));
                        replaced = true;

                    } else if (desc === "(F)V") {
                        var targetName = "setVolume";
                        var n = name.toLowerCase();
                        if (n.indexOf("pitch") !== -1) targetName = "setPitch";
                        else if (n.indexOf("attenuation") !== -1) targetName = "setAttenuation";

                        insns.add(new VarInsnNode(Opcodes.ALOAD, 0));
                        insns.add(new VarInsnNode(Opcodes.FLOAD, 1));
                        insns.add(new MethodInsnNode(Opcodes.INVOKESTATIC, "com/edujime23/decibel/virtual/VirtualChannel", targetName, "(Ljava/lang/Object;F)V", false));
                        insns.add(new InsnNode(Opcodes.RETURN));
                        replaced = true;

                    } else if (desc === "(Z)V") {
                        var targetName = name.toLowerCase().indexOf("loop") !== -1 ? "setLooping" : "setRelative";
                        insns.add(new VarInsnNode(Opcodes.ALOAD, 0));
                        insns.add(new VarInsnNode(Opcodes.ILOAD, 1));
                        insns.add(new MethodInsnNode(Opcodes.INVOKESTATIC, "com/edujime23/decibel/virtual/VirtualChannel", targetName, "(Ljava/lang/Object;Z)V", false));
                        insns.add(new InsnNode(Opcodes.RETURN));
                        replaced = true;

                    } else if (desc === "(I)V") {
                        insns.add(new VarInsnNode(Opcodes.ALOAD, 0));
                        insns.add(new VarInsnNode(Opcodes.ILOAD, 1));
                        insns.add(new MethodInsnNode(Opcodes.INVOKESTATIC, "com/edujime23/decibel/virtual/VirtualChannel", "pumpBuffers", "(Ljava/lang/Object;I)V", false));
                        insns.add(new InsnNode(Opcodes.RETURN));
                        replaced = true;

                    } else if (desc === "(Lnet/minecraft/world/phys/Vec3;)V") {
                        insns.add(new VarInsnNode(Opcodes.ALOAD, 0));
                        insns.add(new VarInsnNode(Opcodes.ALOAD, 1));
                        insns.add(new MethodInsnNode(Opcodes.INVOKEVIRTUAL, "net/minecraft/world/phys/Vec3", "x", "()D", false));
                        insns.add(new VarInsnNode(Opcodes.ALOAD, 1));
                        insns.add(new MethodInsnNode(Opcodes.INVOKEVIRTUAL, "net/minecraft/world/phys/Vec3", "y", "()D", false));
                        insns.add(new VarInsnNode(Opcodes.ALOAD, 1));
                        insns.add(new MethodInsnNode(Opcodes.INVOKEVIRTUAL, "net/minecraft/world/phys/Vec3", "z", "()D", false));
                        insns.add(new MethodInsnNode(Opcodes.INVOKESTATIC, "com/edujime23/decibel/virtual/VirtualChannel", "setSelfPosition", "(Ljava/lang/Object;DDD)V", false));
                        insns.add(new InsnNode(Opcodes.RETURN));
                        replaced = true;

                    } else if (desc === "(Lnet/minecraft/client/sounds/AudioStream;)V") {
                        insns.add(new VarInsnNode(Opcodes.ALOAD, 0));
                        insns.add(new VarInsnNode(Opcodes.ALOAD, 1));
                        insns.add(new MethodInsnNode(Opcodes.INVOKESTATIC, "com/edujime23/decibel/virtual/VirtualChannel", "attachBufferStream", "(Ljava/lang/Object;Lnet/minecraft/client/sounds/AudioStream;)V", false));
                        insns.add(new InsnNode(Opcodes.RETURN));
                        replaced = true;

                    } else if (desc === "(Lcom/mojang/blaze3d/audio/SoundBuffer;)V") {
                        insns.add(new VarInsnNode(Opcodes.ALOAD, 0));
                        insns.add(new VarInsnNode(Opcodes.ALOAD, 1));
                        insns.add(new MethodInsnNode(Opcodes.INVOKESTATIC, "com/edujime23/decibel/virtual/VirtualChannel", "attachStaticBuffer", "(Ljava/lang/Object;Ljava/lang/Object;)V", false));
                        insns.add(new InsnNode(Opcodes.RETURN));
                        replaced = true;
                    }

                    if (replaced) {
                        m.tryCatchBlocks.clear();
                        if (m.localVariables !== null) m.localVariables.clear();
                        m.instructions.clear();
                        m.instructions.add(insns);
                    }
                }
                return classNode;
            }
        }
    };
}